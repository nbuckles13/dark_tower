//! Integration test: telemetry proxy preserves trace context (R-56).
//!
//! POST to `/api/v1/telemetry/v1/metrics` carrying a `traceparent` header →
//! GC's inbound extraction (surface (a)) attaches it as the request span's
//! parent → the telemetry forwarder's outbound request to the collector
//! (surface (b), via `services::telemetry_forwarder`) MUST carry the SAME
//! trace-id in its own `traceparent` header.
//!
//! Harness mirrors `telemetry_proxy_tests.rs`'s `TestTelemetryServer`
//! (trimmed to the happy-path shape only — this test isn't re-exercising the
//! filter/rate-limit/size matrix, just trace propagation through the full
//! router + forwarder).

#![allow(clippy::unwrap_used, clippy::expect_used)]

#[path = "common/mod.rs"]
mod test_common;
use test_common::jwt_fixtures::{TestKeypair, TestUserClaims};
use test_common::otel_support::{known_traceparent, setup_otel_test_environment};

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
use wiremock::{Mock, MockServer, ResponseTemplate};

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

struct TestServer {
    addr: SocketAddr,
    _handle: JoinHandle<()>,
    mock_server: MockServer,
    keypair: TestKeypair,
}

impl TestServer {
    async fn spawn(pool: PgPool) -> Result<Self> {
        let mock_server = MockServer::start().await;
        let keypair = TestKeypair::new(7, "telemetry-otel-key-01");

        let jwks = serde_json::json!({ "keys": [keypair.jwk_json()] });
        Mock::given(method("GET"))
            .and(path("/.well-known/jwks.json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&jwks))
            .mount(&mock_server)
            .await;

        // Collector receiver — 200 for both signals; we only exercise metrics.
        Mock::given(method("POST"))
            .and(path("/v1/metrics"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&mock_server)
            .await;

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
        ]);
        let config = Config::from_vars(&vars)?;

        let (_tx, rx) = watch::channel(SecretString::from("test-token"));
        let token_receiver = TokenReceiver::from_watch_receiver(rx);

        let quota = governor::Quota::per_minute(NonZeroU32::new(60).expect("nonzero"));
        let rate_limiter = Arc::new(governor::RateLimiter::keyed(quota));
        let forwarder =
            TelemetryForwarder::with_timeout(mock_server.uri(), Duration::from_secs(5))?;
        let telemetry = TelemetryState {
            rate_limiter,
            forwarder,
            max_bytes: 256 * 1024,
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
            mock_server,
            keypair,
        })
    }

    fn url(&self) -> String {
        format!("http://{}/api/v1/telemetry/v1/metrics", self.addr)
    }

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

fn empty_metrics_payload() -> Vec<u8> {
    ExportMetricsServiceRequest::default().encode_to_vec()
}

#[sqlx::test(migrations = "../../migrations")]
async fn telemetry_proxy_forward_preserves_inbound_traceparent(pool: PgPool) -> Result<()> {
    // Both the global W3C propagator AND a scoped `OpenTelemetryLayer`
    // subscriber are required here: production's `main.rs` installs both at
    // startup, but a test binary never runs `main()`, so without explicitly
    // installing the subscriber `tracing::Span::current()` inside the
    // extraction middleware / forwarder has no OTel bridge to attach/read a
    // real trace context through — `set_parent`/`.context()` would be
    // effectively inert. The guard is held for the whole test (including
    // across the spawned server task, which runs on the SAME OS thread under
    // `#[sqlx::test]`'s current-thread tokio runtime) so the override stays
    // active for the full request lifecycle.
    let _otel_guard = setup_otel_test_environment();

    let server = TestServer::spawn(pool).await?;
    let token = server.user_token("user-otel");
    let traceparent = known_traceparent();

    let resp = reqwest::Client::new()
        .post(server.url())
        .bearer_auth(token)
        .header("Content-Type", PROTOBUF)
        .header("traceparent", traceparent.clone())
        .body(empty_metrics_payload())
        .send()
        .await?;

    assert_eq!(resp.status(), 202);

    let received = server
        .mock_server
        .received_requests()
        .await
        .expect("recording enabled");
    let collector_request = received
        .iter()
        .find(|r| r.url.path() == "/v1/metrics")
        .expect("collector should have received the forwarded metrics request");

    let forwarded_traceparent = collector_request
        .headers
        .get("traceparent")
        .expect("forwarded request must carry a traceparent header")
        .to_str()
        .expect("valid header string");

    let inbound_trace_id = &traceparent[3..35];
    let forwarded_trace_id = forwarded_traceparent
        .get(3..35)
        .expect("forwarded traceparent has a trace-id segment");
    assert_eq!(
        forwarded_trace_id, inbound_trace_id,
        "telemetry proxy forward to collector must preserve the inbound trace-id"
    );

    Ok(())
}
