// Every `#[sqlx::test]` is implicitly `flavor = "current_thread"`; the local
// Prometheus recorder below is thread-bound, so that pinning is LOAD-BEARING.
//
//! Component test (ADR-0032) for `gc_mh_registrations_total`'s exposed LABEL
//! SHAPE, through the real `MhService::register_mh` recording sites and a real
//! Prometheus exporter render (`MetricAssertion`'s label filter is a subset
//! match, so it cannot prove a label is ABSENT):
//!
//! - `match` appears exactly on `outcome="superseded"`;
//! - no series carries a `handler_id` (or any id/endpoint) label: the only
//!   label keys are `outcome` and `match`;
//! - one increment per registration call.
//!
//! Per-arm deltas (+1 on the expected cell, every other cell unobserved) are in
//! `mh_registration_tests.rs`.

#[path = "common/mod.rs"]
mod test_common;

use std::sync::Arc;

use gc_service::grpc::MhService;
use gc_test_utils::{test_grpc_endpoint, test_mh_endpoint_allowlist, test_wt_endpoint};
use metrics_exporter_prometheus::PrometheusBuilder;
use proto_gen::dark_tower::internal::v1::media_handler_registry_service_server::MediaHandlerRegistryService;
use proto_gen::dark_tower::internal::v1::RegisterMhRequest;
use sqlx::PgPool;
use test_common::mh_registry::{freshness_registry, seed_with_endpoints};
use tonic::Request;

fn req(id: &str, wt: &str, grpc: &str) -> Request<RegisterMhRequest> {
    Request::new(RegisterMhRequest {
        handler_id: id.to_string(),
        region: "test-region".to_string(),
        webtransport_endpoint: wt.to_string(),
        grpc_endpoint: grpc.to_string(),
        max_streams: 1000,
    })
}

#[sqlx::test(migrations = "../../migrations")]
async fn registration_series_label_shape(pool: PgPool) {
    let recorder = PrometheusBuilder::new().build_recorder();
    let handle = recorder.handle();
    let _guard = metrics::set_default_local_recorder(&recorder);

    let svc = MhService::new(
        Arc::new(pool.clone()),
        &freshness_registry(2),
        test_mh_endpoint_allowlist(),
    );
    let (wt_a, grpc_a) = (test_wt_endpoint("a"), test_grpc_endpoint(0, 1));
    let (wt_b, grpc_b) = (test_wt_endpoint("b"), test_grpc_endpoint(0, 2));

    // ok
    svc.register_mh(req("id-ok", &wt_a, &grpc_a)).await.unwrap();
    // superseded{match=both}: same endpoints, new id.
    svc.register_mh(req("id-restart", &wt_a, &grpc_a))
        .await
        .unwrap();
    // endpoint_changed
    svc.register_mh(req("id-restart", &wt_b, &grpc_a))
        .await
        .unwrap_err();
    // address_conflict: a fresh healthy row shares one endpoint.
    seed_with_endpoints(
        &pool,
        "id-live",
        "test-region",
        &wt_b,
        &grpc_b,
        "healthy",
        0,
    )
    .await;
    svc.register_mh(req("id-conflict", &wt_b, &test_grpc_endpoint(0, 3)))
        .await
        .unwrap_err();
    // superseded{match=one_sided}: the live row drains.
    sqlx::query(
        "UPDATE media_handlers SET health_status = 'draining' WHERE handler_id = 'id-live'",
    )
    .execute(&pool)
    .await
    .unwrap();
    svc.register_mh(req("id-conflict", &wt_b, &test_grpc_endpoint(0, 3)))
        .await
        .unwrap();
    // invalid_endpoint
    svc.register_mh(req("id-bad", "http://a.mh.test:4440", &grpc_a))
        .await
        .unwrap_err();

    let rendered = handle.render();
    let mut lines: Vec<&str> = rendered
        .lines()
        .filter(|l| l.starts_with("gc_mh_registrations_total{"))
        .collect();
    lines.sort_unstable();
    assert_eq!(
        lines,
        vec![
            r#"gc_mh_registrations_total{outcome="address_conflict"} 1"#,
            r#"gc_mh_registrations_total{outcome="endpoint_changed"} 1"#,
            r#"gc_mh_registrations_total{outcome="invalid_endpoint"} 1"#,
            r#"gc_mh_registrations_total{outcome="ok"} 1"#,
            r#"gc_mh_registrations_total{outcome="superseded",match="both"} 1"#,
            r#"gc_mh_registrations_total{outcome="superseded",match="one_sided"} 1"#,
        ],
        "exact series and label sets: match only on superseded, no id/endpoint labels"
    );
    for id in [
        "id-ok",
        "id-restart",
        "id-live",
        "id-conflict",
        "id-bad",
        "mh.test",
    ] {
        assert!(
            !lines.iter().any(|l| l.contains(id)),
            "no id or endpoint appears in a label: {id}"
        );
    }
}
