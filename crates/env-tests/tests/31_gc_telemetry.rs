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
//! (`infra/services/otel-collector/collector.yaml`), which logs metric/span
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
//!   See the reverse cross-ref comment in `infra/services/gc-service/config.env`.
//!
//! Edge-sum (concurrent Σ P×(N+M)): 0

#![cfg(feature = "flows")]

use env_tests::cluster::ClusterConnection;
use env_tests::fixtures::auth_client::{
    TokenRequest, UserRegistrationRequest, DEV_TEST_CLIENT_ID, DEV_TEST_CLIENT_SECRET,
};
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
use env_tests::eventual::ConsistencyCategory;
#[cfg(feature = "observability")]
use env_tests::fixtures::metrics::{
    poll_until_any_instance_above, settled_baseline, InstanceCounters,
};
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
/// `test-client` credential (`infra/kind/scripts/deploy.sh:seed_test_data`) is the same one
/// `24_join_flow.rs` uses for its service-token rejection test.
async fn service_token(cluster: &ClusterConnection) -> String {
    let auth = AuthClient::new(&cluster.ac_base_url);
    let request =
        TokenRequest::client_credentials(DEV_TEST_CLIENT_ID, DEV_TEST_CLIENT_SECRET, "test:all");
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
        key_strindex: 0,
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
                entity_refs: vec![],
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
                entity_refs: vec![],
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
                entity_refs: vec![],
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
                                key_strindex: 0,
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
// PII-filter verification helper (C2′ — ratified). Reads GC's drop counter PER
// INSTANCE (`sum by (instance)`, one per GC pod), so the assertion is correct
// regardless of which pod served the POST, and robust to a pod rollover (each
// pod is compared with its OWN baseline; a pod absent from the baseline counts
// from 0). Only compiled under the `observability` feature.
// ============================================================================

/// Bound on the drop-counter baseline settling. The three PII tests run
/// concurrently, and GC pods persist across runs (ADR-0038), so an increment
/// still being scraped would otherwise satisfy a later "rose" check with
/// nothing from this test (a false pass). A failure-only ceiling.
#[cfg(feature = "observability")]
use env_tests::fixtures::metrics::BASELINE_SETTLE_BOUND;

#[cfg(feature = "observability")]
fn pii_dropped_promql(kind: &str) -> String {
    format!("sum by (instance) (gc_telemetry_pii_attributes_dropped_total{{kind=\"{kind}\"}})")
}

/// The SETTLED per-instance baseline of `gc_telemetry_pii_attributes_dropped_total{kind}`:
/// two reads a GC scrape apart agree. The returned map IS the baseline; never re-read.
#[cfg(feature = "observability")]
async fn pii_dropped_baseline(cluster: &ClusterConnection, kind: &str) -> InstanceCounters {
    let prom = PrometheusClient::new(&cluster.prometheus_base_url);
    settled_baseline(
        &prom,
        &pii_dropped_promql(kind),
        BASELINE_SETTLE_BOUND,
        "the PII-filter assertion under test",
    )
    .await
}

/// Assert the `{kind}` drop counter rose past its OWN baseline on some GC pod
/// (the one that served the POST), polling within the `MetricsScrape` budget
/// (scrape lag). Counters are whole numbers, so "rose" is `>= +1`, the one
/// attribute each test plants.
#[cfg(feature = "observability")]
async fn assert_drop_counter_increased(
    cluster: &'static ClusterConnection,
    kind: &'static str,
    before: InstanceCounters,
) {
    let prom = PrometheusClient::new(&cluster.prometheus_base_url);
    let budget = ConsistencyCategory::MetricsScrape.timeout();
    poll_until_any_instance_above(
        &prom,
        &pii_dropped_promql(kind),
        &before,
        budget,
        std::time::Duration::from_secs(2),
        |current| {
            format!(
                "gc_telemetry_pii_attributes_dropped_total{{kind=\"{kind}\"}} did not rise past \
                 its baseline on any GC pod within {budget:?} (baseline {before:?}, now \
                 {current:?}); the deployed PII filter may not have dropped the planted attribute"
            )
        },
    )
    .await;
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
    // (compiled default 60; NOT set in gc-service/config.env today — if a ConfigMap later sets
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
    let before = pii_dropped_baseline(cluster, "resource").await;

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
    assert_drop_counter_increased(cluster, "resource", before).await;
}

/// (h) Non-scalar value on an ALLOWLISTED key (DATAPOINT level) → 202 + the
/// `{kind="datapoint"}` drop counter rises (scalar-only rule).
#[tokio::test]
async fn test_metrics_pii_nonscalar_value_dropped() {
    let cluster = cluster().await;
    let token = shared_user_token(cluster).await;
    let gc = GcClient::new(&cluster.gc_base_url);

    #[cfg(feature = "observability")]
    let before = pii_dropped_baseline(cluster, "datapoint").await;

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
    assert_drop_counter_increased(cluster, "datapoint", before).await;
}

/// (t-c) Non-allowlisted SPAN attribute → 202 + the `{kind="span"}` drop counter
/// rises.
#[tokio::test]
async fn test_traces_pii_nonallowlisted_span_attr_filtered() {
    let cluster = cluster().await;
    let token = shared_user_token(cluster).await;
    let gc = GcClient::new(&cluster.gc_base_url);

    #[cfg(feature = "observability")]
    let before = pii_dropped_baseline(cluster, "span").await;

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
    assert_drop_counter_increased(cluster, "span", before).await;
}

// ============================================================================
// Story 3 R-29 (+ `content_kind`, 2026-10-26): the collector's EXACT kind
// value rules, end to end
// ============================================================================

/// The kind label keys (pinned against `common` in the hygiene kernel).
#[cfg(feature = "observability")]
use env_tests::fixtures::metric_hygiene::{CONTENT_KIND_KEY, MEDIA_KIND_KEY};

/// An EXPORTED, per-kind client name that NO alert or browser read-back
/// selects on, written at value 0. Not `frames_received`: that feeds
/// `MCMediaMissingKeyMaterial` and the R-29 browser read-back, and a synthetic
/// `media_kind="video"` series living there for `metric_expiration` would let a
/// later video read-back pass vacuously. A gauge also sidesteps
/// `delta_to_cumulative` temporality.
#[cfg(feature = "observability")]
const MEDIA_KIND_PROBE_METRIC: &str = "dt_client_media_send_queue_depth";

/// One gauge datapoint at 0 whose `client_version` is a per-case MARKER (kept
/// and shape-checked by the collector), so the read-back selects only this
/// run's case and a stale series from an earlier run cannot satisfy it.
#[cfg(feature = "observability")]
fn media_kind_probe_point(
    key: &str,
    companion: (&str, &str),
    marker: &str,
    kind: Option<Value>,
) -> NumberDataPoint {
    let mut attributes = vec![
        string_attr("client_version", marker),
        string_attr("key_custody", "operator"),
    ];
    if let Some(v) = kind {
        // The other kind key, valid: a LONE kind key is poisoned to
        // invalid/invalid by the collector's pair rules (all or none), which
        // would mask the value rule under test.
        attributes.push(string_attr(companion.0, companion.1));
        attributes.push(KeyValue {
            key_strindex: 0,
            key: key.to_string(),
            value: Some(AnyValue { value: Some(v) }),
        });
    }
    NumberDataPoint {
        attributes,
        time_unix_nano: now_unix_ns(),
        value: Some(opentelemetry_proto::tonic::metrics::v1::number_data_point::Value::AsInt(0)),
        ..Default::default()
    }
}

/// (r29) The collector's `media_kind` value rule FIRES and APPLIES: posted
/// through the real GC proxy (user JWT) into the deployed Kind collector, a
/// `video` value reaches Prometheus intact, a `bogus` one arrives as `invalid`
/// (and never as `bogus`), and a non-string one arrives as `invalid`.
///
/// GC deliberately does not value-check `media_kind`, so the collector rule is
/// the only value bound; this is the proof it is live in the deployed config
/// (the Kind overlay loads `infra/services/otel-collector/collector.yaml`
/// verbatim through its content-addressed `configMapGenerator`).
///
/// Observability-only: without Prometheus this proves nothing, so it does not
/// degrade to a 202-only check.
#[cfg(feature = "observability")]
#[tokio::test]
async fn test_metrics_media_kind_value_rule_fires_and_applies() {
    kind_value_rule_fires_and_applies(
        MEDIA_KIND_KEY,
        "video",
        (CONTENT_KIND_KEY, "main"),
        "mk",
        "^(audio|video)$",
    )
    .await;
}

/// (2026-10-26) The `content_kind` twin: same three cases plus `absent`, under
/// the collector's exact `^(main|screen)$` rule. Each probe also carries a
/// valid `media_kind` (a lone key is poisoned by the pair rules); only the
/// probed key is asserted.
#[cfg(feature = "observability")]
#[tokio::test]
async fn test_metrics_content_kind_value_rule_fires_and_applies() {
    kind_value_rule_fires_and_applies(
        CONTENT_KIND_KEY,
        "main",
        (MEDIA_KIND_KEY, "audio"),
        "ck",
        "^(main|screen)$",
    )
    .await;
}

#[cfg(feature = "observability")]
async fn kind_value_rule_fires_and_applies(
    key: &str,
    good: &str,
    companion: (&str, &str),
    tag: &str,
    rule: &str,
) {
    let cluster = cluster().await;
    let token = shared_user_token(cluster).await;
    let gc = GcClient::new(&cluster.gc_base_url);

    let run = &uuid::Uuid::new_v4().simple().to_string()[..8];
    // (case, sent value, expected stored value). `absent` stands in for the
    // KIND-FREE session series (KEK, roster, capture source): the collector rule
    // is not name-scoped, so a probe sent with NO media_kind proves the
    // `!= nil` conjunct live — it must arrive with no media_kind at all, never
    // `invalid`.
    let cases: [(&str, Option<Value>, Option<&str>); 4] = [
        (good, Some(Value::StringValue(good.into())), Some(good)),
        (
            "bogus",
            Some(Value::StringValue("bogus".into())),
            Some("invalid"),
        ),
        ("int", Some(Value::IntValue(7)), Some("invalid")),
        ("absent", None, None),
    ];
    let markers: Vec<String> = cases
        .iter()
        .map(|(name, _, _)| format!("et31-{tag}-{name}-{run}"))
        .collect();

    let body = ExportMetricsServiceRequest {
        resource_metrics: vec![ResourceMetrics {
            resource: None,
            scope_metrics: vec![ScopeMetrics {
                scope: None,
                metrics: vec![Metric {
                    name: MEDIA_KIND_PROBE_METRIC.to_string(),
                    data: Some(Data::Gauge(Gauge {
                        data_points: cases
                            .iter()
                            .zip(&markers)
                            .map(|((_, v, _), m)| {
                                media_kind_probe_point(key, companion, m, v.clone())
                            })
                            .collect(),
                    })),
                    ..Default::default()
                }],
                schema_url: String::new(),
            }],
            schema_url: String::new(),
        }],
    }
    .encode_to_vec();

    let resp = gc
        .raw_ingest_telemetry("metrics", Some(token), Some(PROTOBUF_CT), body)
        .await
        .expect("network request should succeed");
    assert_eq!(
        resp.status().as_u16(),
        202,
        "media_kind probe POST must be accepted"
    );

    let prom = PrometheusClient::new(&cluster.prometheus_base_url);
    let selector = format!(
        "{MEDIA_KIND_PROBE_METRIC}{{client_version=~\"{}\"}}",
        markers.join("|")
    );
    // Positive control: every marker's series arrived. Only then are the
    // per-case value checks (including "bogus is absent") meaningful.
    let arrived =
        env_tests::eventual::assert_eventually(ConsistencyCategory::MetricsScrape, || {
            let prom = PrometheusClient::new(&cluster.prometheus_base_url);
            let selector = selector.clone();
            let markers = markers.clone();
            async move {
                prom.query_promql(&selector).await.ok().is_some_and(|r| {
                    markers.iter().all(|m| {
                        r.data
                            .result
                            .iter()
                            .any(|s| s.metric.get("client_version") == Some(m))
                    })
                })
            }
        })
        .await;
    assert!(
        arrived.is_ok(),
        "media-kind-probe-not-stored: the marker series {markers:?} never reached \
         Prometheus via GC -> collector -> scrape. The value-rule assertions would be \
         vacuous; investigate the collector name filter / scrape before the rule."
    );

    let result = prom
        .query_promql(&selector)
        .await
        .expect("Prometheus query should succeed")
        .data
        .result;
    for ((name, _, want), marker) in cases.iter().zip(&markers) {
        let mine: Vec<_> = result
            .iter()
            .filter(|s| s.metric.get("client_version") == Some(marker))
            .collect();
        let kinds: Vec<Option<&String>> = mine.iter().map(|s| s.metric.get(key)).collect();
        assert_eq!(
            kinds,
            vec![want.map(str::to_string).as_ref()],
            "media-kind-value-rule: case `{name}` must arrive as exactly one series with \
             {key}={want:?}; got {kinds:?}. `bogus`/int surviving means the \
             collector's exact `{rule}` rewrite is not in the deployed config; \
             `{good}` rewritten means the rule is over-tight; `absent` acquiring a value \
             means the rule's `!= nil` guard is broken and kind-free series are being \
             stamped `invalid`."
        );
    }
    assert!(
        !result
            .iter()
            .any(|s| s.metric.get(key).map(String::as_str) == Some("bogus")),
        "media-kind-value-rule: a `{key}=\"bogus\"` series was stored"
    );
}

// ============================================================================
// Story 3 OBS-B: FIRE + APPLY for the client video alert rules
// (ClientVideoServerMutedSenderFrames A5, ClientVideoDecodeErrorRatio A6,
// ClientVideoSlotUnfulfilled A9)
// ============================================================================

/// The client alert rules file the FIRE probes derive their expressions from.
#[cfg(feature = "observability")]
const CLIENT_RULES_FILE: &str = "client-alerts.yaml";

#[cfg(feature = "observability")]
fn now_unix_ns() -> u64 {
    u64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock after epoch")
            .as_nanos(),
    )
    .expect("timestamp fits u64")
}

/// Datapoint attributes for a FIRE post: the per-run marker, the constant
/// custody, the video kind and (when the selector needs it) a reason. Nothing
/// else — no identity of any kind.
#[cfg(feature = "observability")]
fn fire_attrs(marker: &str, reason: Option<&str>) -> Vec<KeyValue> {
    let mut v = vec![
        string_attr("client_version", marker),
        string_attr("key_custody", "operator"),
        string_attr(env_tests::fixtures::metric_hygiene::MEDIA_KIND_KEY, "video"),
        // The video alerts select the camera (`content_kind="main"`) since
        // 2026-10-26 (label-taxonomy.md §query rule).
        string_attr(
            env_tests::fixtures::metric_hygiene::CONTENT_KIND_KEY,
            "main",
        ),
    ];
    if let Some(r) = reason {
        v.push(string_attr("reason", r));
    }
    v
}

/// One DELTA monotonic Sum datapoint, as the SDK exports a counter.
#[cfg(feature = "observability")]
fn delta_counter(name: &str, value: i64, attrs: Vec<KeyValue>, start: u64, time: u64) -> Metric {
    use opentelemetry_proto::tonic::metrics::v1::{number_data_point, AggregationTemporality, Sum};
    Metric {
        name: name.to_string(),
        data: Some(Data::Sum(Sum {
            data_points: vec![NumberDataPoint {
                attributes: attrs,
                start_time_unix_nano: start,
                time_unix_nano: time,
                value: Some(number_data_point::Value::AsInt(value)),
                ..Default::default()
            }],
            aggregation_temporality: AggregationTemporality::Delta as i32,
            is_monotonic: true,
        })),
        ..Default::default()
    }
}

/// One DELTA histogram datapoint with `count` observations of `value` each.
#[cfg(feature = "observability")]
fn delta_histogram(
    name: &str,
    bounds: &[f64],
    value: f64,
    count: u64,
    attrs: Vec<KeyValue>,
    start: u64,
    time: u64,
) -> Metric {
    use opentelemetry_proto::tonic::metrics::v1::{
        AggregationTemporality, Histogram, HistogramDataPoint,
    };
    let idx = bounds
        .iter()
        .position(|b| value <= *b)
        .unwrap_or(bounds.len());
    let mut bucket_counts = vec![0_u64; bounds.len() + 1];
    bucket_counts[idx] = count;
    Metric {
        name: name.to_string(),
        data: Some(Data::Histogram(Histogram {
            data_points: vec![HistogramDataPoint {
                attributes: attrs,
                start_time_unix_nano: start,
                time_unix_nano: time,
                count,
                sum: Some(value * count as f64),
                bucket_counts,
                explicit_bounds: bounds.to_vec(),
                ..Default::default()
            }],
            aggregation_temporality: AggregationTemporality::Delta as i32,
        })),
        ..Default::default()
    }
}

/// `SLOT_UNFULFILLED_BUCKETS_MS` read out of the SDK source (its SSoT).
/// COULD-NOT-EVALUATE if the const cannot be found or parsed.
#[cfg(feature = "observability")]
fn sdk_slot_unfulfilled_bounds() -> Vec<f64> {
    use env_tests::fixtures::alert_rules_loaded::{ts_numeric_array_const, SDK_MEDIA_METRICS_TS};
    let path = env_tests::repo_root().join(SDK_MEDIA_METRICS_TS);
    let src = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("COULD-NOT-EVALUATE: cannot read {}: {e}", path.display()));
    ts_numeric_array_const(&src, "SLOT_UNFULFILLED_BUCKETS_MS")
        .unwrap_or_else(|e| panic!("COULD-NOT-EVALUATE: {SDK_MEDIA_METRICS_TS}: {e}"))
}

/// (story 3 OBS-B) The client video alert rules FIRE and APPLY on the deployed
/// pipeline: GC proxy (user JWT) -> collector (`delta_to_cumulative`) ->
/// Prometheus scrape -> the rule's OWN expression.
///
/// For each rule the committed expr is read from `client-alerts.yaml`, and ONE
/// matcher (`client_version="<per-run marker>"`) is added to each `dt_client_*`
/// selector by the span-based kernel — nothing else changes, and the MC gauge
/// selector A9 compares against is untouched. So the marked expr can be
/// satisfied only by THIS run's posts: it must be EMPTY before them and
/// NON-EMPTY after. The `for:` duration is never waited out, and no latency
/// number is asserted: the A9 observation value is derived from the live MC
/// gauge (twice the rule's bound), never a literal.
///
/// Env-tests are Rust, so the "patched client" of A5 (a receiver reporting
/// `sender_server_muted` drops) is this authenticated OTLP post of the
/// counter, the same bytes a browser would send.
///
/// Counters and histograms are DELTA, as the SDK exports them; two posts are
/// sent, the second only after the first is visible in Prometheus (so they land
/// in different scrapes), so `rate()` sees an increase. Two requests, each
/// carrying all four series (A5's drops, A6's decoder errors and accepted
/// frames, A9's histogram), well inside GC's per-user rate limit.
///
/// This is the FIRE of the client pipeline. The client rules' APPLY is carried
/// by `dt-guard client-metrics-export` (names, label keys),
/// `packages/web-app/tests/clientMetricNames.test.ts` (names, label values) and
/// the browser read-backs (the live pipe).
#[cfg(feature = "observability")]
#[tokio::test]
async fn test_story3_client_video_alerts_fire_and_apply() {
    use env_tests::fixtures::alert_rules_loaded::{committed_alert_expr, inject_marker};
    use env_tests::fixtures::metrics::{
        eventually_returns_series, mc_video_min_keyframe_interval_seconds,
    };

    let cluster = cluster().await;
    let token = shared_user_token(cluster).await;
    let gc = GcClient::new(&cluster.gc_base_url);
    let prom = PrometheusClient::new(&cluster.prometheus_base_url);

    let run = &uuid::Uuid::new_v4().simple().to_string()[..8];
    let cases = [
        ("ClientVideoServerMutedSenderFrames", "a5"),
        ("ClientVideoDecodeErrorRatio", "a6"),
        ("ClientVideoSlotUnfulfilled", "a9"),
    ];
    let mut marked = Vec::new();
    for (alert, short) in cases {
        let expr = committed_alert_expr(CLIENT_RULES_FILE, alert);
        let marker = format!("et31-fire-{short}-{run}");
        let (m, n) = inject_marker(&expr, "dt_client_", "client_version", &marker)
            .unwrap_or_else(|e| panic!("COULD-NOT-EVALUATE: marking `{alert}`: {e}"));
        assert!(
            n >= 1,
            "COULD-NOT-EVALUATE: `{alert}` has no dt_client_ selector to mark: {expr}"
        );
        // Precondition: nothing but this run's (not yet sent) posts can match.
        let pre = prom
            .query_promql(&m)
            .await
            .unwrap_or_else(|e| panic!("COULD-NOT-EVALUATE: `{m}` failed: {e}"));
        assert!(
            pre.data.result.is_empty(),
            "FIRE-PRECONDITION-NOT-EMPTY: `{alert}`'s marked expression already returns a \
             series before this run posted anything; the marker is per run, so the probe is \
             not isolated: {m}"
        );
        marked.push((alert, marker, m));
    }

    // A9's observation: twice the rule's bound, derived from the LIVE gauge.
    let interval = mc_video_min_keyframe_interval_seconds(&prom).await;
    let a9_value_ms = 2.0 * 10.0 * 1000.0 * interval;
    let bounds = sdk_slot_unfulfilled_bounds();

    let body = |start: u64, time: u64| -> Vec<u8> {
        let m = |i: usize| marked[i].1.as_str();
        ExportMetricsServiceRequest {
            resource_metrics: vec![ResourceMetrics {
                resource: None,
                scope_metrics: vec![ScopeMetrics {
                    scope: None,
                    metrics: vec![
                        delta_counter(
                            "dt_client_media_frames_dropped_total",
                            10,
                            fire_attrs(m(0), Some("sender_server_muted")),
                            start,
                            time,
                        ),
                        delta_counter(
                            "dt_client_media_decoder_errors_total",
                            10,
                            fire_attrs(m(1), None),
                            start,
                            time,
                        ),
                        delta_counter(
                            "dt_client_media_frames_accepted_total",
                            100,
                            fire_attrs(m(1), None),
                            start,
                            time,
                        ),
                        delta_histogram(
                            "dt_client_media_slot_unfulfilled_duration_ms",
                            &bounds,
                            a9_value_ms,
                            20,
                            fire_attrs(m(2), None),
                            start,
                            time,
                        ),
                    ],
                    schema_url: String::new(),
                }],
                schema_url: String::new(),
            }],
        }
        .encode_to_vec()
    };

    let t0 = now_unix_ns();
    let first = body(t0.saturating_sub(1_000_000_000), t0);
    let resp = gc
        .raw_ingest_telemetry("metrics", Some(token), Some(PROTOBUF_CT), first)
        .await
        .expect("network request should succeed");
    assert_eq!(resp.status().as_u16(), 202, "FIRE post 1 must be accepted");

    // The first post is visible (so the second lands in a LATER scrape).
    let first_marker = &marked[0].1;
    let visible = eventually_returns_series(
        &prom,
        &format!(r#"dt_client_media_frames_dropped_total{{client_version="{first_marker}"}}"#),
    )
    .await;
    assert!(
        visible,
        "FIRE-NOT-OBSERVED: the first post never reached Prometheus via GC -> collector -> \
         scrape; investigate the pipe before the rules"
    );

    let t1 = now_unix_ns();
    let second = body(t0, t1);
    let resp = gc
        .raw_ingest_telemetry("metrics", Some(token), Some(PROTOBUF_CT), second)
        .await
        .expect("network request should succeed");
    assert_eq!(resp.status().as_u16(), 202, "FIRE post 2 must be accepted");

    for (alert, _, m) in &marked {
        let fired = eventually_returns_series(&prom, m).await;
        assert!(
            fired,
            "FIRE-NOT-OBSERVED: `{alert}` — its committed expression from {CLIENT_RULES_FILE}, \
             with exactly one client_version marker matcher added to each dt_client selector \
             and nothing else, returned no series after two posts of an adverse condition. \
             Either the rule cannot fire on real client data (a selector/label the pipe does not \
             carry, or a threshold the condition cannot reach) or the posts were not stored: \
             {m}"
        );
    }
}
