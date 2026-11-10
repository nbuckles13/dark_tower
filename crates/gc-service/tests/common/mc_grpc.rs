//! Fixtures for driving GC's `McService` (MC→GC gRPC) directly in
//! integration tests: an `AppState` over a real pool, and the meeting
//! ownership rows `AcquireMediaHandler` checks. The canonical home: do not copy.

use common::secret::SecretString;
use common::token_manager::TokenReceiver;
use gc_service::config::Config;
use gc_service::handlers::TelemetryState;
use gc_service::repositories::MeetingControllersRepository;
use gc_service::routes::AppState;
use gc_service::services::MockMcClient;
use sqlx::PgPool;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::watch;

/// `AppState` with the shipped MH registry values (gc-test-utils), optionally
/// overriding `GC_MH_MAX_EXCLUDE_HANDLER_IDS` (tests ABOUT the cap pass a
/// non-default value, so a hardcoded 16 cannot pass them).
pub fn mc_app_state(pool: PgPool, max_exclude_handler_ids: Option<u32>) -> Arc<AppState> {
    let mut vars = HashMap::from([
        (
            "DATABASE_URL".to_string(),
            "postgresql://test/test".to_string(),
        ),
        ("GC_CLIENT_ID".to_string(), "test-gc-client".to_string()),
        ("GC_CLIENT_SECRET".to_string(), "test-gc-secret".to_string()),
    ]);
    vars.extend(gc_test_utils::mh_config_vars());
    if let Some(cap) = max_exclude_handler_ids {
        vars.insert("GC_MH_MAX_EXCLUDE_HANDLER_IDS".to_string(), cap.to_string());
    }
    let config = Config::from_vars(&vars).expect("config should load");
    let (_tx, rx) = watch::channel(SecretString::from("test-token"));
    let telemetry = TelemetryState::from_config(
        config.otel_collector_endpoint.clone(),
        config.telemetry_proxy_max_bytes,
        config.telemetry_proxy_rate_limit_per_minute,
    )
    .expect("telemetry state");
    Arc::new(AppState {
        pool,
        config,
        mc_client: Arc::new(MockMcClient::accepting()),
        token_receiver: TokenReceiver::from_watch_receiver(rx),
        telemetry,
    })
}

/// Register a meeting controller row (the FK target of an assignment).
pub async fn seed_mc(pool: &PgPool, controller_id: &str, region: &str) {
    MeetingControllersRepository::register_mc(
        pool,
        controller_id,
        region,
        &format!("http://{controller_id}:50051"),
        None,
        100,
        1000,
    )
    .await
    .expect("register mc");
}

/// Insert an assignment row for (meeting, region) owned by `controller_id`;
/// `ended` sets `ended_at`.
pub async fn seed_assignment(
    pool: &PgPool,
    meeting_id: &str,
    controller_id: &str,
    region: &str,
    ended: bool,
) {
    sqlx::query(
        "INSERT INTO meeting_assignments \
         (meeting_id, meeting_controller_id, region, assigned_by_gc_id, ended_at) \
         VALUES ($1, $2, $3, 'gc-test', CASE WHEN $4 THEN NOW() ELSE NULL END)",
    )
    .bind(meeting_id)
    .bind(controller_id)
    .bind(region)
    .bind(ended)
    .execute(pool)
    .await
    .expect("seed assignment");
}

/// Set an MH row's reported load (`current_streams` of `max_streams`).
pub async fn set_load(pool: &PgPool, handler_id: &str, current: i32, max: i32) {
    sqlx::query(
        "UPDATE media_handlers SET current_streams = $2, max_streams = $3 WHERE handler_id = $1",
    )
    .bind(handler_id)
    .bind(current)
    .bind(max)
    .execute(pool)
    .await
    .expect("set load");
}
