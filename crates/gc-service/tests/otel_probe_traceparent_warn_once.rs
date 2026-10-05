//! A `traceparent` sent to a probe path (whose request span is DEBUG and so
//! disabled under the deployed filter) is context someone sent and GC
//! dropped: it warns, once per process.
//!
//! Its own test binary with a single test: the warn-once latch behind
//! `common::observability::otel::set_remote_parent` is process-global, so
//! sharing a binary with any other traceparent-bearing request would make
//! the count order-dependent.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use common::observability::testing::otel::{
    deployed_rust_log, install_test_propagator, known_traceparent, SpanCapture,
};

use axum::body::Body;
use axum::http::Request;
use common::secret::SecretString;
use common::token_manager::TokenReceiver;
use gc_service::config::Config;
use gc_service::handlers::TelemetryState;
use gc_service::routes::{self, AppState};
use gc_service::services::MockMcClient;
use sqlx::postgres::PgPoolOptions;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::watch;
use tower::ServiceExt;

fn router() -> axum::Router {
    let vars = HashMap::from([
        (
            "DATABASE_URL".to_string(),
            "postgresql://test/test".to_string(),
        ),
        ("GC_CLIENT_ID".to_string(), "test-gc-client".to_string()),
        ("GC_CLIENT_SECRET".to_string(), "test-gc-secret".to_string()),
    ]);
    let config = Config::from_vars(&vars).expect("config should load");
    let (_tx, rx) = watch::channel(SecretString::from("test-token"));
    let telemetry = TelemetryState::from_config(
        config.otel_collector_endpoint.clone(),
        config.telemetry_proxy_max_bytes,
        config.telemetry_proxy_rate_limit_per_minute,
    )
    .expect("telemetry state");
    let state = Arc::new(AppState {
        pool: PgPoolOptions::new()
            .connect_lazy("postgresql://test/test")
            .expect("lazy pool"),
        config,
        mc_client: Arc::new(MockMcClient::accepting()),
        token_receiver: TokenReceiver::from_watch_receiver(rx),
        telemetry,
    });
    let metrics_handle = metrics_exporter_prometheus::PrometheusBuilder::new()
        .build_recorder()
        .handle();
    routes::build_routes(state, metrics_handle).expect("build_routes")
}

#[tokio::test(flavor = "current_thread")]
async fn traceparent_on_probe_path_warns_exactly_once() {
    install_test_propagator();
    let capture = SpanCapture::install_with_filter(&deployed_rust_log(
        "infra/services/gc-service/deployment.yaml",
    ));

    for _ in 0..2 {
        let request = Request::builder()
            .uri("/health")
            .header("traceparent", known_traceparent())
            .body(Body::empty())
            .unwrap();
        drop(router().oneshot(request).await.unwrap());
    }

    assert_eq!(
        capture.extraction_warns(),
        1,
        "two traceparent-bearing probe requests must warn exactly once"
    );
}
