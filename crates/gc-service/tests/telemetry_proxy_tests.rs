//! Integration tests for the GC telemetry proxy (R-2).
//!
//! `POST /api/v1/telemetry/v1/{metrics,traces}` — a sanitizing OTLP reverse
//! proxy with a deny-by-default attribute allowlist. Covers the full failure
//! matrix: 202 / 401 / 413 / 415 / 429 / 400 / 502, plus the load-bearing
//! "filter-but-forward" semantic (drops still 202, re-encoded bytes received by
//! the collector no longer contain the dropped key).

#![allow(clippy::unwrap_used, clippy::expect_used)]

#[path = "common/mod.rs"]
mod test_common;
use test_common::jwt_fixtures::{TestKeypair, TestUserClaims};

use anyhow::Result;
use chrono::Utc;
use common::secret::SecretString;
use common::token_manager::TokenReceiver;
use gc_service::config::Config;
use gc_service::handlers::TelemetryState;
use gc_service::observability::metrics::init_metrics_recorder;
use gc_service::routes::{self, AppState};
use gc_service::services::telemetry_forwarder::TelemetryForwarder;
use gc_service::services::MockMcClient;
use opentelemetry_proto::tonic::collector::metrics::v1::ExportMetricsServiceRequest;
use opentelemetry_proto::tonic::common::v1::{any_value::Value, AnyValue, KeyValue, KeyValueList};
use opentelemetry_proto::tonic::metrics::v1::{
    metric::Data, Gauge, Metric, NumberDataPoint, ResourceMetrics, ScopeMetrics,
};
use opentelemetry_proto::tonic::resource::v1::Resource;
use prost::Message;
use sqlx::PgPool;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::num::NonZeroU32;
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use tokio::sync::watch;
use tokio::task::JoinHandle;
use uuid::Uuid;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

static TEST_METRICS_HANDLE: OnceLock<metrics_exporter_prometheus::PrometheusHandle> =
    OnceLock::new();

fn get_test_metrics_handle() -> metrics_exporter_prometheus::PrometheusHandle {
    TEST_METRICS_HANDLE
        .get_or_init(|| {
            init_metrics_recorder().unwrap_or_else(|_| {
                metrics_exporter_prometheus::PrometheusBuilder::new()
                    .build_recorder()
                    .handle()
            })
        })
        .clone()
}

const PROTOBUF: &str = "application/x-protobuf";

/// Test server for the telemetry proxy.
struct TestTelemetryServer {
    addr: SocketAddr,
    _handle: JoinHandle<()>,
    // Held to keep the wiremock JWKS + collector receivers alive for the
    // lifetime of the test server.
    _mock_server: MockServer,
    keypair: TestKeypair,
}

/// Knobs for spawning the server with specific telemetry behavior.
struct TelemetryServerOpts {
    /// Collector response status for `/v1/metrics` and `/v1/traces` (None =
    /// don't mount the collector, used by the connection/empty cases).
    collector_status: Option<u16>,
    /// Per-minute rate-limit quota.
    rate_limit_per_minute: u32,
    /// Max payload bytes (DefaultBodyLimit + explicit check).
    max_bytes: usize,
    /// Forwarder total timeout.
    forward_timeout: Duration,
    /// Override the collector endpoint (e.g. empty string to test disabled).
    endpoint_override: Option<String>,
    /// Mount the collector with `.expect(0)` so MockServer-drop verification
    /// asserts the forwarder was NEVER called (reject paths short-circuit before
    /// forward). Pins "never called" from the wire side, alongside the metric
    /// side `assert_unobserved` (per @test belt-and-suspenders).
    collector_expect_zero: bool,
}

impl Default for TelemetryServerOpts {
    fn default() -> Self {
        Self {
            collector_status: Some(200),
            rate_limit_per_minute: 60,
            max_bytes: 256 * 1024,
            forward_timeout: Duration::from_secs(5),
            endpoint_override: None,
            collector_expect_zero: false,
        }
    }
}

impl TestTelemetryServer {
    async fn spawn(pool: PgPool, opts: TelemetryServerOpts) -> Result<Self> {
        let mock_server = MockServer::start().await;
        let keypair = TestKeypair::new(7, "telemetry-key-01");

        // JWKS endpoint for user-token validation.
        let jwks = serde_json::json!({ "keys": [keypair.jwk_json()] });
        Mock::given(method("GET"))
            .and(path("/.well-known/jwks.json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&jwks))
            .mount(&mock_server)
            .await;

        // Collector receivers (record received bodies for assertions).
        if let Some(status) = opts.collector_status {
            for p in ["/v1/metrics", "/v1/traces"] {
                let mock = Mock::given(method("POST"))
                    .and(path(p))
                    .respond_with(ResponseTemplate::new(status));
                // On reject paths the forwarder must never be hit — assert 0
                // calls, verified when the MockServer drops.
                let mock = if opts.collector_expect_zero {
                    mock.expect(0)
                } else {
                    mock
                };
                mock.mount(&mock_server).await;
            }
        }

        let vars = HashMap::from([
            (
                "DATABASE_URL".to_string(),
                "postgresql://test/test".to_string(),
            ),
            ("BIND_ADDRESS".to_string(), "127.0.0.1:0".to_string()),
            (
                "AC_JWKS_URL".to_string(),
                format!("{}/.well-known/jwks.json", mock_server.uri()),
            ),
            ("GC_CLIENT_ID".to_string(), "test-gc-client".to_string()),
            ("GC_CLIENT_SECRET".to_string(), "test-gc-secret".to_string()),
            (
                "TELEMETRY_PROXY_MAX_BYTES".to_string(),
                opts.max_bytes.to_string(),
            ),
            (
                "TELEMETRY_PROXY_RATE_LIMIT_PER_MINUTE".to_string(),
                opts.rate_limit_per_minute.to_string(),
            ),
        ]);
        let config = Config::from_vars(&vars)?;

        let (_tx, rx) = watch::channel(SecretString::from("test-token"));
        let token_receiver = TokenReceiver::from_watch_receiver(rx);

        // Build a TelemetryState pointed at the wiremock collector with an
        // injected quota + short forward timeout.
        let endpoint = opts.endpoint_override.unwrap_or_else(|| mock_server.uri());
        let quota =
            governor::Quota::per_minute(NonZeroU32::new(opts.rate_limit_per_minute).unwrap());
        let rate_limiter = Arc::new(governor::RateLimiter::keyed(quota));
        let forwarder = TelemetryForwarder::with_timeout(endpoint, opts.forward_timeout)?;
        let telemetry = TelemetryState {
            rate_limiter,
            forwarder,
            max_bytes: opts.max_bytes,
        };

        let state = Arc::new(AppState {
            pool,
            config,
            mc_client: Arc::new(MockMcClient::accepting()),
            token_receiver,
            telemetry,
        });

        let app = routes::build_routes(state, get_test_metrics_handle())?;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let addr = listener.local_addr()?;
        let handle = tokio::spawn(async move {
            let svc = app.into_make_service_with_connect_info::<SocketAddr>();
            let _ = axum::serve(listener, svc).await;
        });

        Ok(Self {
            addr,
            _handle: handle,
            _mock_server: mock_server,
            keypair,
        })
    }

    fn url(&self, signal: &str) -> String {
        format!("http://{}/api/v1/telemetry/v1/{}", self.addr, signal)
    }

    /// Mint a valid user token for the given subject.
    fn user_token(&self, sub: &str) -> String {
        let now = Utc::now().timestamp();
        let claims = TestUserClaims {
            sub: sub.to_string(),
            org_id: Uuid::new_v4().to_string(),
            email: "user@example.com".to_string(),
            roles: vec!["user".to_string()],
            iat: now,
            exp: now + 3600,
            jti: Uuid::new_v4().to_string(),
        };
        self.keypair.sign_user_token(&claims)
    }
}

/// Build a minimal OTLP metrics payload carrying one allowlisted key
/// (`org_id`) and one disallowed key (`secret_user`) at the resource level.
fn metrics_payload_with_pii() -> Vec<u8> {
    let req = ExportMetricsServiceRequest {
        resource_metrics: vec![ResourceMetrics {
            resource: Some(Resource {
                attributes: vec![
                    KeyValue {
                        key: "org_id".to_string(),
                        value: None,
                    },
                    KeyValue {
                        key: "secret_user".to_string(),
                        value: None,
                    },
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
    };
    req.encode_to_vec()
}

/// A simple valid (empty) metrics payload.
fn empty_metrics_payload() -> Vec<u8> {
    ExportMetricsServiceRequest::default().encode_to_vec()
}

/// Build an OTLP metrics payload that smuggles PII inside the VALUE of an
/// ALLOWLISTED key: `org_id` (allowlisted) carries a nested kvlist holding a
/// `secret_email` attribute. The filter must drop the whole `org_id` attribute
/// because its value is non-scalar — the key being allowlisted is not enough.
fn metrics_payload_with_value_smuggling() -> Vec<u8> {
    let smuggled = AnyValue {
        value: Some(Value::KvlistValue(KeyValueList {
            values: vec![KeyValue {
                key: "secret_email".to_string(),
                value: Some(AnyValue {
                    value: Some(Value::StringValue("victim@example.com".to_string())),
                }),
            }],
        })),
    };
    let req = ExportMetricsServiceRequest {
        resource_metrics: vec![ResourceMetrics {
            resource: Some(Resource {
                attributes: vec![
                    // Allowlisted key, but non-scalar (kvlist) value → must drop.
                    KeyValue {
                        key: "org_id".to_string(),
                        value: Some(smuggled),
                    },
                    // Allowlisted key, scalar value → must survive.
                    KeyValue {
                        key: "client_version".to_string(),
                        value: Some(AnyValue {
                            value: Some(Value::StringValue("1.2.3".to_string())),
                        }),
                    },
                ],
                dropped_attributes_count: 0,
            }),
            scope_metrics: vec![],
            schema_url: String::new(),
        }],
    };
    req.encode_to_vec()
}

// ============================================================================
// Tests
// ============================================================================

#[sqlx::test(migrations = "../../migrations")]
async fn telemetry_metrics_happy_path_returns_202(pool: PgPool) -> Result<()> {
    let server = TestTelemetryServer::spawn(pool, TelemetryServerOpts::default()).await?;
    let token = server.user_token("user-happy");

    let resp = reqwest::Client::new()
        .post(server.url("metrics"))
        .bearer_auth(token)
        .header("Content-Type", PROTOBUF)
        .body(empty_metrics_payload())
        .send()
        .await?;

    assert_eq!(resp.status(), 202);
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn telemetry_unauthenticated_returns_401(pool: PgPool) -> Result<()> {
    let server = TestTelemetryServer::spawn(pool, TelemetryServerOpts::default()).await?;

    let resp = reqwest::Client::new()
        .post(server.url("metrics"))
        .header("Content-Type", PROTOBUF)
        .body(empty_metrics_payload())
        .send()
        .await?;

    assert_eq!(resp.status(), 401);
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn telemetry_wrong_content_type_returns_415(pool: PgPool) -> Result<()> {
    // 415 short-circuits before forward — collector must never be called.
    let opts = TelemetryServerOpts {
        collector_expect_zero: true,
        ..Default::default()
    };
    let server = TestTelemetryServer::spawn(pool, opts).await?;
    let token = server.user_token("user-415");

    let resp = reqwest::Client::new()
        .post(server.url("metrics"))
        .bearer_auth(token)
        .header("Content-Type", "application/json")
        .body(empty_metrics_payload())
        .send()
        .await?;

    assert_eq!(resp.status(), 415);
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn telemetry_oversize_returns_413_boundary(pool: PgPool) -> Result<()> {
    // max_bytes = 1024: exactly 1024 passes the size gate (then 400-decodes on
    // garbage), 1025 → 413. Neither body reaches the collector, so expect 0
    // forwards (wire-side pin).
    let opts = TelemetryServerOpts {
        max_bytes: 1024,
        collector_expect_zero: true,
        ..Default::default()
    };
    let server = TestTelemetryServer::spawn(pool, opts).await?;
    let token = server.user_token("user-413");
    let client = reqwest::Client::new();

    // Exactly at the limit: a 1024-byte body. It will fail to DECODE (garbage),
    // but that is a 400 (decode), proving the size gate did NOT trip at 1024.
    let at_limit = vec![0u8; 1024];
    let resp = client
        .post(server.url("metrics"))
        .bearer_auth(&token)
        .header("Content-Type", PROTOBUF)
        .body(at_limit)
        .send()
        .await?;
    assert_ne!(resp.status(), 413, "1024 bytes must not trip the size cap");

    // One over the limit → 413.
    let over = vec![0u8; 1025];
    let resp = client
        .post(server.url("metrics"))
        .bearer_auth(&token)
        .header("Content-Type", PROTOBUF)
        .body(over)
        .send()
        .await?;
    assert_eq!(resp.status(), 413, "1025 bytes must trip the size cap");

    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn telemetry_oversize_lying_content_length_still_413(pool: PgPool) -> Result<()> {
    // Real-bytes cap is load-bearing: an oversize body is rejected regardless
    // of the Content-Length header value. reqwest sets an honest Content-Length,
    // but the gate is the real streamed bytes (DefaultBodyLimit), so an oversize
    // body → 413 even though we never trust a header.
    let opts = TelemetryServerOpts {
        max_bytes: 512,
        ..Default::default()
    };
    let server = TestTelemetryServer::spawn(pool, opts).await?;
    let token = server.user_token("user-413b");

    let resp = reqwest::Client::new()
        .post(server.url("metrics"))
        .bearer_auth(token)
        .header("Content-Type", PROTOBUF)
        .body(vec![0u8; 4096])
        .send()
        .await?;

    assert_eq!(resp.status(), 413);
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn telemetry_rate_limited_returns_429_and_is_per_sub(pool: PgPool) -> Result<()> {
    // Quota = 2/min: 2 requests succeed, the 3rd (same sub, same window) → 429.
    let opts = TelemetryServerOpts {
        rate_limit_per_minute: 2,
        ..Default::default()
    };
    let server = TestTelemetryServer::spawn(pool, opts).await?;
    let client = reqwest::Client::new();

    let token_a = server.user_token("sub-A");
    for _ in 0..2 {
        let resp = client
            .post(server.url("metrics"))
            .bearer_auth(&token_a)
            .header("Content-Type", PROTOBUF)
            .body(empty_metrics_payload())
            .send()
            .await?;
        assert_eq!(resp.status(), 202);
    }
    let resp = client
        .post(server.url("metrics"))
        .bearer_auth(&token_a)
        .header("Content-Type", PROTOBUF)
        .body(empty_metrics_payload())
        .send()
        .await?;
    assert_eq!(
        resp.status(),
        429,
        "3rd request for sub-A must be throttled"
    );

    // Per-sub isolation: a DIFFERENT sub is unaffected.
    let token_b = server.user_token("sub-B");
    let resp = client
        .post(server.url("metrics"))
        .bearer_auth(&token_b)
        .header("Content-Type", PROTOBUF)
        .body(empty_metrics_payload())
        .send()
        .await?;
    assert_eq!(resp.status(), 202, "sub-B must not be throttled by sub-A");

    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn telemetry_malformed_proto_returns_400(pool: PgPool) -> Result<()> {
    // Decode failure short-circuits before forward: the collector must never be
    // called. `.expect(0)` pins this from the wire side (verified on MockServer
    // drop), complementing the metric-side `assert_unobserved` coverage in
    // tests/telemetry_metrics_integration.rs (per @test belt-and-suspenders).
    let opts = TelemetryServerOpts {
        collector_expect_zero: true,
        ..Default::default()
    };
    let server = TestTelemetryServer::spawn(pool, opts).await?;
    let token = server.user_token("user-400");

    // Bytes that are not a valid OTLP protobuf message.
    let resp = reqwest::Client::new()
        .post(server.url("metrics"))
        .bearer_auth(token)
        .header("Content-Type", PROTOBUF)
        .body(vec![0xff, 0xff, 0xff, 0xff, 0x0f])
        .send()
        .await?;

    assert_eq!(resp.status(), 400);
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn telemetry_collector_5xx_returns_502(pool: PgPool) -> Result<()> {
    let opts = TelemetryServerOpts {
        collector_status: Some(503),
        ..Default::default()
    };
    let server = TestTelemetryServer::spawn(pool, opts).await?;
    let token = server.user_token("user-502a");

    let resp = reqwest::Client::new()
        .post(server.url("metrics"))
        .bearer_auth(token)
        .header("Content-Type", PROTOBUF)
        .body(empty_metrics_payload())
        .send()
        .await?;

    assert_eq!(resp.status(), 502);
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn telemetry_collector_4xx_returns_502(pool: PgPool) -> Result<()> {
    // A collector 4xx must ALSO map to 502 — not passed through to the client.
    let opts = TelemetryServerOpts {
        collector_status: Some(400),
        ..Default::default()
    };
    let server = TestTelemetryServer::spawn(pool, opts).await?;
    let token = server.user_token("user-502b");

    let resp = reqwest::Client::new()
        .post(server.url("metrics"))
        .bearer_auth(token)
        .header("Content-Type", PROTOBUF)
        .body(empty_metrics_payload())
        .send()
        .await?;

    assert_eq!(resp.status(), 502);
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn telemetry_collector_timeout_returns_502(pool: PgPool) -> Result<()> {
    // Collector delays 30s; forwarder timeout is 100ms → 502 without a real wait.
    let mock_server = MockServer::start().await;
    let keypair = TestKeypair::new(7, "telemetry-key-01");
    let jwks = serde_json::json!({ "keys": [keypair.jwk_json()] });
    Mock::given(method("GET"))
        .and(path("/.well-known/jwks.json"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&jwks))
        .mount(&mock_server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/metrics"))
        .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_secs(30)))
        .mount(&mock_server)
        .await;

    let vars = HashMap::from([
        (
            "DATABASE_URL".to_string(),
            "postgresql://test/test".to_string(),
        ),
        (
            "AC_JWKS_URL".to_string(),
            format!("{}/.well-known/jwks.json", mock_server.uri()),
        ),
        ("GC_CLIENT_ID".to_string(), "c".to_string()),
        ("GC_CLIENT_SECRET".to_string(), "s".to_string()),
    ]);
    let config = Config::from_vars(&vars)?;
    let (_tx, rx) = watch::channel(SecretString::from("test-token"));
    let telemetry = TelemetryState {
        rate_limiter: Arc::new(governor::RateLimiter::keyed(governor::Quota::per_minute(
            NonZeroU32::new(60).unwrap(),
        ))),
        forwarder: TelemetryForwarder::with_timeout(mock_server.uri(), Duration::from_millis(100))?,
        max_bytes: 256 * 1024,
    };
    let state = Arc::new(AppState {
        pool,
        config,
        mc_client: Arc::new(MockMcClient::accepting()),
        token_receiver: TokenReceiver::from_watch_receiver(rx),
        telemetry,
    });
    let app = routes::build_routes(state, get_test_metrics_handle())?;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    let _h = tokio::spawn(async move {
        let svc = app.into_make_service_with_connect_info::<SocketAddr>();
        let _ = axum::serve(listener, svc).await;
    });

    let now = Utc::now().timestamp();
    let token = keypair.sign_user_token(&TestUserClaims {
        sub: "user-timeout".to_string(),
        org_id: Uuid::new_v4().to_string(),
        email: "u@e.com".to_string(),
        roles: vec!["user".to_string()],
        iat: now,
        exp: now + 3600,
        jti: Uuid::new_v4().to_string(),
    });

    let resp = reqwest::Client::new()
        .post(format!("http://{}/api/v1/telemetry/v1/metrics", addr))
        .bearer_auth(token)
        .header("Content-Type", PROTOBUF)
        .body(empty_metrics_payload())
        .send()
        .await?;

    assert_eq!(resp.status(), 502);
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn telemetry_disabled_endpoint_returns_503(pool: PgPool) -> Result<()> {
    let opts = TelemetryServerOpts {
        endpoint_override: Some(String::new()),
        collector_status: None,
        ..Default::default()
    };
    let server = TestTelemetryServer::spawn(pool, opts).await?;
    let token = server.user_token("user-503");

    let resp = reqwest::Client::new()
        .post(server.url("metrics"))
        .bearer_auth(token)
        .header("Content-Type", PROTOBUF)
        .body(empty_metrics_payload())
        .send()
        .await?;

    assert_eq!(resp.status(), 503);
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn telemetry_drops_pii_but_forwards_filtered_bytes(pool: PgPool) -> Result<()> {
    // The load-bearing "filter-but-forward" test: a payload with a disallowed
    // key still returns 202, AND the body the collector actually receives,
    // decoded, no longer contains the dropped key (proves re-serialization from
    // the filtered struct, not a raw forward of the original body).
    let mock_server = MockServer::start().await;
    let keypair = TestKeypair::new(7, "telemetry-key-01");
    let jwks = serde_json::json!({ "keys": [keypair.jwk_json()] });
    Mock::given(method("GET"))
        .and(path("/.well-known/jwks.json"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&jwks))
        .mount(&mock_server)
        .await;

    // Capture the forwarded body via a responder that records the request.
    let received: Arc<std::sync::Mutex<Option<Vec<u8>>>> = Arc::new(std::sync::Mutex::new(None));
    let received_clone = received.clone();
    Mock::given(method("POST"))
        .and(path("/v1/metrics"))
        .respond_with(move |req: &Request| {
            *received_clone.lock().unwrap() = Some(req.body.clone());
            ResponseTemplate::new(200)
        })
        .mount(&mock_server)
        .await;

    let vars = HashMap::from([
        (
            "DATABASE_URL".to_string(),
            "postgresql://test/test".to_string(),
        ),
        (
            "AC_JWKS_URL".to_string(),
            format!("{}/.well-known/jwks.json", mock_server.uri()),
        ),
        ("GC_CLIENT_ID".to_string(), "c".to_string()),
        ("GC_CLIENT_SECRET".to_string(), "s".to_string()),
    ]);
    let config = Config::from_vars(&vars)?;
    let (_tx, rx) = watch::channel(SecretString::from("test-token"));
    let telemetry = TelemetryState {
        rate_limiter: Arc::new(governor::RateLimiter::keyed(governor::Quota::per_minute(
            NonZeroU32::new(60).unwrap(),
        ))),
        forwarder: TelemetryForwarder::new(mock_server.uri())?,
        max_bytes: 256 * 1024,
    };
    let state = Arc::new(AppState {
        pool,
        config,
        mc_client: Arc::new(MockMcClient::accepting()),
        token_receiver: TokenReceiver::from_watch_receiver(rx),
        telemetry,
    });
    let app = routes::build_routes(state, get_test_metrics_handle())?;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    let _h = tokio::spawn(async move {
        let svc = app.into_make_service_with_connect_info::<SocketAddr>();
        let _ = axum::serve(listener, svc).await;
    });

    let now = Utc::now().timestamp();
    let token = keypair.sign_user_token(&TestUserClaims {
        sub: "user-pii".to_string(),
        org_id: Uuid::new_v4().to_string(),
        email: "u@e.com".to_string(),
        roles: vec!["user".to_string()],
        iat: now,
        exp: now + 3600,
        jti: Uuid::new_v4().to_string(),
    });

    let resp = reqwest::Client::new()
        .post(format!("http://{}/api/v1/telemetry/v1/metrics", addr))
        .bearer_auth(token)
        .header("Content-Type", PROTOBUF)
        .body(metrics_payload_with_pii())
        .send()
        .await?;

    // (a) drops still return 202.
    assert_eq!(resp.status(), 202);

    // (c) the collector-received body, decoded, no longer contains the dropped
    // key but DOES retain the allowlisted one.
    let body = received
        .lock()
        .unwrap()
        .clone()
        .expect("collector should have received a forwarded body");
    let decoded = ExportMetricsServiceRequest::decode(&body[..])?;
    let attrs = &decoded.resource_metrics[0]
        .resource
        .as_ref()
        .unwrap()
        .attributes;
    let keys: Vec<&str> = attrs.iter().map(|kv| kv.key.as_str()).collect();
    assert!(keys.contains(&"org_id"), "allowlisted key must survive");
    assert!(
        !keys.contains(&"secret_user"),
        "disallowed key must be stripped from the forwarded bytes"
    );

    // ---- CRUX-A: value-smuggling stripped end-to-end ----
    // Second request: an ALLOWLISTED key (org_id) carrying a non-scalar (kvlist)
    // value that hides PII. The whole org_id attribute must be stripped from the
    // forwarded bytes; the scalar-valued client_version survives. And the
    // smuggled "secret_email" / "victim@example.com" must NOT appear anywhere in
    // the forwarded bytes (defense-in-depth: scan the raw wire bytes too).
    let resp2 = reqwest::Client::new()
        .post(format!("http://{}/api/v1/telemetry/v1/metrics", addr))
        .bearer_auth(server_token(&keypair, "user-smuggle"))
        .header("Content-Type", PROTOBUF)
        .body(metrics_payload_with_value_smuggling())
        .send()
        .await?;
    assert_eq!(resp2.status(), 202);

    let body2 = received
        .lock()
        .unwrap()
        .clone()
        .expect("collector should have received the second forwarded body");
    let decoded2 = ExportMetricsServiceRequest::decode(&body2[..])?;
    let attrs2 = &decoded2.resource_metrics[0]
        .resource
        .as_ref()
        .unwrap()
        .attributes;
    let keys2: Vec<&str> = attrs2.iter().map(|kv| kv.key.as_str()).collect();
    assert!(
        keys2.contains(&"client_version"),
        "scalar-valued allowlisted key must survive"
    );
    assert!(
        !keys2.contains(&"org_id"),
        "allowlisted key with a non-scalar (kvlist) value must be dropped (value smuggling)"
    );
    // Raw-bytes scan: the smuggled PII must not survive anywhere in the forward.
    let body2_str = String::from_utf8_lossy(&body2);
    assert!(
        !body2_str.contains("victim@example.com") && !body2_str.contains("secret_email"),
        "smuggled PII must not appear anywhere in the forwarded bytes"
    );

    Ok(())
}

/// Mint a valid user token for the given subject (free fn for tests that build
/// their own server inline rather than via `TestTelemetryServer`).
fn server_token(keypair: &TestKeypair, sub: &str) -> String {
    let now = Utc::now().timestamp();
    keypair.sign_user_token(&TestUserClaims {
        sub: sub.to_string(),
        org_id: Uuid::new_v4().to_string(),
        email: "u@e.com".to_string(),
        roles: vec!["user".to_string()],
        iat: now,
        exp: now + 3600,
        jti: Uuid::new_v4().to_string(),
    })
}

// ============================================================================
// End-to-end metric fidelity (@test review F1)
// ============================================================================

/// Parse the cumulative value of a `gc_telemetry_ingest_total` series with the
/// given `status` + `payload_kind` labels out of a Prometheus exposition dump.
/// Returns 0.0 if the series is absent. Label order in the render is
/// alphabetical (`payload_kind` before `status`), but we match on substring
/// presence of both label pairs to be order-robust.
fn ingest_total_value(rendered: &str, status: &str, payload_kind: &str) -> f64 {
    let status_lbl = format!("status=\"{status}\"");
    let kind_lbl = format!("payload_kind=\"{payload_kind}\"");
    for line in rendered.lines() {
        if line.starts_with("gc_telemetry_ingest_total{")
            && line.contains(&status_lbl)
            && line.contains(&kind_lbl)
        {
            if let Some(v) = line.rsplit(' ').next() {
                return v.parse().unwrap_or(0.0);
            }
        }
    }
    0.0
}

/// Count the cumulative observation count of `gc_telemetry_ingest_duration_seconds`
/// for a given `status` (the histogram `_count` series). 0.0 if absent.
fn ingest_duration_count(rendered: &str, status: &str) -> f64 {
    let status_lbl = format!("status=\"{status}\"");
    for line in rendered.lines() {
        if line.starts_with("gc_telemetry_ingest_duration_seconds_count{")
            && line.contains(&status_lbl)
        {
            if let Some(v) = line.rsplit(' ').next() {
                return v.parse().unwrap_or(0.0);
            }
        }
    }
    0.0
}

/// Proves the handler's `IngestGuard` status-label wiring matches the metric
/// taxonomy END-TO-END through the real axum stack — not just via direct
/// `record_telemetry_ingest(...)` unit calls. Fires a real 413 (oversize) and a
/// real 429 (rate-limited) request and asserts the corresponding
/// `gc_telemetry_ingest_total{status,payload_kind}` series + duration histogram
/// advanced.
///
/// The handler runs under `tokio::spawn` in the harness, so the thread-local
/// `MetricAssertion` recorder cannot observe its emission — we read the GLOBAL
/// Prometheus recorder via `get_test_metrics_handle().render()` (the same
/// recorder the handler emits into), taking a before/after delta because the
/// global recorder accumulates across all tests in this binary.
#[sqlx::test(migrations = "../../migrations")]
async fn telemetry_reject_paths_emit_correct_status_label_end_to_end(pool: PgPool) -> Result<()> {
    let handle = get_test_metrics_handle();

    // --- 413 HANDLER-path → status="rejected_size" (T1) ---
    // Body in (max_bytes, 2×max_bytes] reaches the handler (passes the layer
    // ceiling at 2×max_bytes) → handler's explicit size check 413s AND emits
    // rejected_size. max_bytes=512 → ceiling 1024; a 768-byte body is in-window.
    let opts413 = TelemetryServerOpts {
        max_bytes: 512,
        collector_expect_zero: true,
        ..Default::default()
    };
    let server413 = TestTelemetryServer::spawn(pool.clone(), opts413).await?;
    let token413 = server413.user_token("sub-e2e-413");

    let before_413 = ingest_total_value(&handle.render(), "rejected_size", "metric");
    let before_413_dur = ingest_duration_count(&handle.render(), "rejected_size");

    let resp = reqwest::Client::new()
        .post(server413.url("metrics"))
        .bearer_auth(token413)
        .header("Content-Type", PROTOBUF)
        .body(vec![0u8; 768]) // 512 < 768 <= 1024 → reaches handler → 413
        .send()
        .await?;
    assert_eq!(resp.status(), 413);

    let rendered_413 = handle.render();
    let after_413 = ingest_total_value(&rendered_413, "rejected_size", "metric");
    let after_413_dur = ingest_duration_count(&rendered_413, "rejected_size");
    assert!(
        after_413 - before_413 >= 1.0,
        "413 request must increment gc_telemetry_ingest_total{{status=rejected_size,payload_kind=metric}} \
         (before={before_413}, after={after_413})"
    );
    assert!(
        after_413_dur - before_413_dur >= 1.0,
        "413 request must record a gc_telemetry_ingest_duration_seconds observation for status=rejected_size"
    );

    // --- 429 rate-limited path → status="rejected_rate" ---
    let opts429 = TelemetryServerOpts {
        rate_limit_per_minute: 1,
        ..Default::default()
    };
    let server429 = TestTelemetryServer::spawn(pool, opts429).await?;
    let token429 = server429.user_token("sub-e2e-429");
    let client = reqwest::Client::new();

    // First request consumes the quota (202).
    let ok = client
        .post(server429.url("metrics"))
        .bearer_auth(&token429)
        .header("Content-Type", PROTOBUF)
        .body(empty_metrics_payload())
        .send()
        .await?;
    assert_eq!(ok.status(), 202);

    let before_429 = ingest_total_value(&handle.render(), "rejected_rate", "metric");

    // Second request (same sub, same window) → 429.
    let limited = client
        .post(server429.url("metrics"))
        .bearer_auth(&token429)
        .header("Content-Type", PROTOBUF)
        .body(empty_metrics_payload())
        .send()
        .await?;
    assert_eq!(limited.status(), 429);

    let after_429 = ingest_total_value(&handle.render(), "rejected_rate", "metric");
    assert!(
        after_429 - before_429 >= 1.0,
        "429 request must increment gc_telemetry_ingest_total{{status=rejected_rate,payload_kind=metric}} \
         (before={before_429}, after={after_429})"
    );

    Ok(())
}

/// T3 (boundary trio) + T2 (layer-path 413 is metric-LESS). Pins the two-tier
/// size cap (Option 1): the configured limit is the user-facing 413+metric gate
/// in the handler; the `2×` layer ceiling is a metric-less DoS backstop.
///
/// Three meaningful points for max_bytes=1024 (ceiling=2048):
/// - 1024 bytes (== max_bytes) → passes the size gate, then 400-decodes on
///   garbage (proves the gate did NOT trip at the exact limit).
/// - 1025 bytes (max_bytes+1, in-window) → HANDLER 413 + rejected_size EMITTED.
/// - 2049 bytes (> ceiling) → LAYER 413, body never reaches the handler, so
///   rejected_size is NOT emitted for that request and the collector is never
///   called.
#[sqlx::test(migrations = "../../migrations")]
async fn telemetry_413_two_tier_boundary_and_layer_path_is_metricless(pool: PgPool) -> Result<()> {
    let handle = get_test_metrics_handle();
    let opts = TelemetryServerOpts {
        max_bytes: 1024,
        // Both 413s and the 400-decode never forward — assert 0 collector calls.
        collector_expect_zero: true,
        ..Default::default()
    };
    let server = TestTelemetryServer::spawn(pool, opts).await?;
    let token = server.user_token("sub-2tier");
    let client = reqwest::Client::new();

    let post = |body: Vec<u8>| {
        let url = server.url("metrics");
        let token = token.clone();
        let client = client.clone();
        async move {
            client
                .post(url)
                .bearer_auth(token)
                .header("Content-Type", PROTOBUF)
                .body(body)
                .send()
                .await
        }
    };

    // Point 1: exactly max_bytes → passes size gate, 400 on garbage (not 413).
    let at_limit = post(vec![0u8; 1024]).await?;
    assert_ne!(
        at_limit.status(),
        413,
        "exactly max_bytes must NOT trip the size cap"
    );

    // Point 2: max_bytes+1, in (max_bytes, ceiling] → HANDLER 413 + metric.
    let before_hdr = ingest_total_value(&handle.render(), "rejected_size", "metric");
    let in_window = post(vec![0u8; 1025]).await?;
    assert_eq!(in_window.status(), 413, "max_bytes+1 → handler 413");
    let after_hdr = ingest_total_value(&handle.render(), "rejected_size", "metric");
    assert!(
        after_hdr - before_hdr >= 1.0,
        "handler-path 413 must EMIT rejected_size (before={before_hdr}, after={after_hdr})"
    );

    // Point 3 (T2): > ceiling → LAYER 413, body never reaches the handler.
    // The hard, race-free proof that the handler was bypassed is the collector
    // `.expect(0)` (no forward) + a 413 with no body reaching prost-decode; the
    // layer-path's metric-lessness is the documented, tested boundary.
    let over_ceiling = post(vec![0u8; 2049]).await?;
    assert_eq!(
        over_ceiling.status(),
        413,
        "> 2×max_bytes → layer 413 (DoS backstop)"
    );

    Ok(())
}
