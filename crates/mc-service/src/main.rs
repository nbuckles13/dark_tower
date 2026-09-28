//! Meeting Controller
//!
//! Stateful WebTransport signaling server for real-time meeting coordination.
//!
//! # Servers
//!
//! The Meeting Controller runs multiple servers:
//! - WebTransport server for client signaling (default: 0.0.0.0:4433)
//! - gRPC server for GC communication (default: 0.0.0.0:50052)
//! - HTTP server for health endpoints (default: 0.0.0.0:8081)
//!
//! # Architecture (ADR-0023)
//!
//! Uses an actor model hierarchy:
//! - `MeetingControllerActor` (singleton): Supervises meetings
//! - `MeetingActor` (per meeting): Owns meeting state
//! - `ParticipantActor` (per participant): Handles one participant in a meeting
//!
//! # State Management
//!
//! - Live state in Redis with sync writes for critical data
//! - Fencing tokens prevent split-brain during failover
//! - Session binding tokens enable secure reconnection
//!
//! # Startup Flow (ADR-0023 Phase 6c, ADR-0010)
//!
//! 1. Load configuration from environment
//! 2. Initialize Prometheus metrics recorder (ADR-0011)
//! 3. Initialize Redis connection (`FencedRedisClient`)
//! 4. Spawn `TokenManager` for OAuth token acquisition from AC (ADR-0010)
//! 5. Initialize actor system (`MeetingControllerActorHandle`)
//! 6. Start health HTTP server (liveness, readiness, metrics)
//! 7. Start gRPC server for GC->MC communication
//! 8. Create `GcClient` with `TokenReceiver` and spawn GC task (registration + heartbeats)
//! 9. Wait for shutdown signal

#![warn(clippy::pedantic)]
#![allow(clippy::too_many_lines)] // main.rs orchestrates startup, naturally longer

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use common::secret::{ExposeSecret, SecretBox};
use common::token_manager::{spawn_token_manager, TokenManagerConfig};
use mc_service::actors::{ActorMetrics, ControllerMetrics, MeetingControllerActorHandle};
use mc_service::auth::McJwtValidator;
use mc_service::config::Config;
use mc_service::errors::McError;
use mc_service::grpc::gc_client::{MeetingEndedQueue, MEETING_ENDED_QUEUE_CAPACITY};
use mc_service::grpc::{
    GcClient, McAssignmentService, McAuthLayer, McMediaCoordinationService, MhClient,
    MhRegistrationClient,
};
use mc_service::observability::{health_router, HealthState};
use mc_service::redis::FencedRedisClient;
use mc_service::system_info::gather_system_info;
use mc_service::webtransport::WebTransportServer;
use proto_gen::dark_tower::internal::v1::media_coordination_service_server::MediaCoordinationServiceServer;
use proto_gen::dark_tower::internal::v1::meeting_controller_service_server::MeetingControllerServiceServer;
use proto_gen::dark_tower::internal::v1::HealthStatus;
use tokio::signal;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

/// Default timeout for initial token acquisition.
const TOKEN_ACQUISITION_TIMEOUT: Duration = Duration::from_secs(30);

/// Minimum secret length for HMAC-SHA256 (32 bytes).
const MIN_SECRET_LENGTH: usize = 32;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load configuration BEFORE the tracing subscriber: the R-55 OTel layer
    // needs the endpoint/sample-rate from config, and the subscriber can only be
    // `.init()`'d once. A config-load failure here has no subscriber to log
    // through yet — it propagates as a non-zero exit via the default Debug-to-
    // stderr dump (same as the AC/GC/MH exemplars); nothing is lost because no
    // spans/logs have been emitted yet.
    let config = Config::from_env()?;

    // R-55: initialize the OpenTelemetry SDK only when explicitly enabled
    // (OTEL_ENABLED=true). `init_otel` is async, eagerly probes the collector,
    // and fails hard at init on an unreachable/misconfigured endpoint so the pod
    // fails K8s readiness instead of silently dropping spans. When OTel is
    // disabled, `otel_config()` returns `None`, `init_otel` is never called (no
    // probe), and no OTel layer is composed.
    let otel = match config.otel_config() {
        Some(otel_cfg) => Some(
            common::observability::otel::init_otel(
                "meeting-controller",
                env!("CARGO_PKG_VERSION"),
                &config.environment,
                otel_cfg,
            )
            .await?,
        ),
        None => None,
    };
    // Split into the composable layer and the RAII guard. `_otel_guard` is held
    // to the end of `main` so pending spans flush on shutdown via Drop; it is a
    // NAMED binding (not bare `_`) so it is not dropped immediately.
    let (otel_layer, _otel_guard) = match otel {
        Some(init) => (Some(init.layer), Some(init.guard)),
        None => (None, None),
    };

    // Initialize tracing with JSON structured logging. The OTel layer (if any)
    // composes onto the bare registry first (its type is Layer<Registry> only);
    // the default-off path adds zero layers. JSON format enables robust parsing
    // in Promtail without brittle regex.
    tracing_subscriber::registry()
        .with(otel_layer)
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "mc_service=debug,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer().json())
        .init();

    info!("Starting Meeting Controller");

    info!(
        region = %config.region,
        mc_id = %config.mc_id,
        webtransport_bind_address = %config.webtransport_bind_address,
        grpc_bind_address = %config.grpc_bind_address,
        health_bind_address = %config.health_bind_address,
        max_meetings = config.max_meetings,
        max_participants = config.max_participants,
        binding_token_ttl_seconds = config.binding_token_ttl_seconds,
        // ADR-0036 §5/§6 client-signalling knobs, logged as EFFECTIVE values.
        //
        // The ConfigMap cannot answer "what did this process actually receive?" —
        // the ConfigMap being out of step with what the Deployment injects IS the
        // failure mode these required keys create, so the artifact under
        // suspicion cannot also be the evidence. A missing key is loud
        // (CrashLoop, `logs --previous` names it); a WRONG-BUT-VALID value is
        // silent, and this line is the only record of it. Same discipline as
        // mh-service, which logs every effective transport value and its derived
        // drain window in one structured startup line.
        max_receive_slots = config.max_receive_slots,
        max_receive_capability_declarations = config.max_receive_capability_declarations,
        audio_codec = ?config.audio_encoding.codec(),
        audio_max_bitrate_bps = config.audio_encoding.max_bitrate_bps(),
        audio_frame_rate_hz = config.audio_encoding.frame_rate_hz(),
        // DERIVED teardown bounds (story 2 task 12), logged so the runbook names
        // a field instead of restating arithmetic over four constants in two
        // files. Named for what each bounds, not for its formula.
        push_quiesce_bound_seconds =
            mc_service::media_routing::teardown::PUSH_QUIESCE_BOUND.as_secs(),
        teardown_fence_hold_max_seconds =
            mc_service::media_routing::teardown::TEARDOWN_MAX.as_secs(),
        // The third derived bound, and the only one whose inputs include a value
        // copied from the pod manifest — so the only one that can go wrong from
        // OUTSIDE this crate. It decides whether a release happens at all, where
        // the two above shape one teardown's duration. Logged HERE rather than
        // only at shutdown because the shutdown line carrying it is the
        // cut-off branch: a healthy exit would never print it.
        shutdown_release_budget_seconds =
            mc_service::media_routing::teardown::SHUTDOWN_RELEASE_BUDGET.as_secs(),
        "Configuration loaded successfully"
    );

    // A frame rate other than 50 Hz is LEGAL and deliberately reachable —
    // ADR-0036 §3 names 40 ms frames (25 Hz) as the signature-overhead
    // mitigation, so pinning this to 50 would block a designed route. But
    // mh-service sizes its datagram send buffer in BYTES against 20 ms frames,
    // and MC cannot validate against that constant (it is private, and importing
    // mh-service would be worse than the problem). So the available control is
    // to make the divergence LOUD at the moment an operator can still act on it,
    // rather than leaving it to a comment in a file they may never open.
    // Compared against the MH SIZING anchor, never against the band ceiling.
    // The two are equal today; anchoring on the ceiling would invert this
    // warning the moment the band is widened — see the constant's rustdoc.
    if config.audio_encoding.frame_rate_hz()
        != mc_service::media_signaling::AUDIO_FRAME_RATE_MH_SIZING_HZ
    {
        warn!(
            audio_frame_rate_hz = config.audio_encoding.frame_rate_hz(),
            expected_by_media_handler_sizing =
                mc_service::media_signaling::AUDIO_FRAME_RATE_MH_SIZING_HZ,
            "MC is directing an audio frame rate other than 20 ms frames. mh-service sizes its \
             datagram send buffer against 20 ms frames, so its reported buffer latency will \
             UNDERSTATE the real held latency until its AUDIO_FRAME_DURATION_MS is confirmed to \
             match. This is a coordinated change with media-handler, not a unilateral one."
        );
    }

    // Initialize Prometheus metrics recorder (ADR-0011)
    // This must happen before any metrics are recorded
    info!("Initializing Prometheus metrics recorder...");
    let prometheus_handle =
        mc_service::observability::metrics::init_metrics_recorder().map_err(|e| {
            error!(error = %e, "Failed to install Prometheus metrics recorder");
            e
        })?;
    // Zero-initialize discrete-event counters so each series is present at 0 from
    // process start (counter-visibility fix — a lazily-created counter has no
    // 0→1 edge and `increase()` reads 0 forever). Infallible; must run right
    // after the recorder installs and before any event can fire.
    mc_service::observability::metrics::zero_initialize_counters();
    info!("Prometheus metrics recorder initialized");

    // Initialize health state
    let health_state = Arc::new(HealthState::new());

    // Initialize Redis connection (Phase 6b)
    info!("Connecting to Redis...");
    let redis_client = FencedRedisClient::new(config.redis_url.expose_secret())
        .await
        .map_err(|e| {
            error!(error = %e, "Failed to connect to Redis");
            e
        })?;
    let redis_client = Arc::new(redis_client);
    info!("Redis connection established");

    // Spawn TokenManager for OAuth token acquisition (ADR-0010)
    info!(
        ac_endpoint = %config.ac_endpoint,
        client_id = %config.client_id,
        "Spawning TokenManager for AC authentication..."
    );

    // Use from_url() to automatically handle HTTP (local dev) or HTTPS (production)
    let token_config = TokenManagerConfig::from_url(
        config.ac_endpoint.clone(),
        config.client_id.clone(),
        config.client_secret.clone(),
    )
    .map_err(|e| {
        error!(error = %e, "Failed to create TokenManager config");
        McError::TokenAcquisition(format!("TokenManager config error: {e}"))
    })?
    .with_on_refresh(Arc::new(|event| {
        mc_service::observability::metrics::record_token_refresh_metrics(&event);
    }));

    let (token_task_handle, token_rx) =
        tokio::time::timeout(TOKEN_ACQUISITION_TIMEOUT, spawn_token_manager(token_config))
            .await
            .map_err(|_| {
                error!(
                    timeout_secs = TOKEN_ACQUISITION_TIMEOUT.as_secs(),
                    "Token acquisition timed out - AC may be unreachable"
                );
                McError::TokenAcquisitionTimeout
            })?
            .map_err(|e| {
                error!(error = %e, "Failed to acquire initial token from AC");
                McError::TokenAcquisition(format!("Initial token acquisition failed: {e}"))
            })?;

    info!("TokenManager spawned successfully, initial token acquired");

    // Initialize JWKS client + JWT validator for meeting token validation (ADR-0020)
    // This validates client meeting/guest tokens presented during WebTransport join.
    // Separate from gRPC McAuthLayer which validates service-to-service tokens.
    info!(
        ac_jwks_url = %config.ac_jwks_url,
        "Initializing JWKS client for meeting token validation..."
    );
    let jwks_client = Arc::new(
        common::jwt::JwksClient::new(config.ac_jwks_url.clone()).map_err(|e| {
            error!(error = %e, "Failed to create JWKS client");
            McError::Config(format!("JWKS client initialization failed: {e}"))
        })?,
    );

    #[expect(
        clippy::cast_possible_wrap,
        reason = "clock_skew_seconds bounded to <=600, safe u64->i64"
    )]
    let jwt_validator = Arc::new(McJwtValidator::new(
        Arc::clone(&jwks_client),
        config.clock_skew_seconds as i64,
    ));
    info!("JWKS client initialized for meeting token validation");

    // Initialize shared metrics for heartbeat reporting
    let controller_metrics = ControllerMetrics::new();

    // Initialize actor system (Phase 6b)
    info!("Initializing actor system...");
    let actor_metrics = ActorMetrics::new();

    // Decode master secret for session binding tokens from base64 config
    let master_secret = {
        use base64::Engine;
        let decoder = base64::engine::general_purpose::STANDARD;
        let secret_bytes = decoder
            .decode(config.binding_token_secret.expose_secret())
            .map_err(|e| {
                error!(error = %e, "MC_BINDING_TOKEN_SECRET is not valid base64");
                format!("Invalid base64 in MC_BINDING_TOKEN_SECRET: {e}")
            })?;

        if secret_bytes.len() < MIN_SECRET_LENGTH {
            error!(
                length = secret_bytes.len(),
                min_length = MIN_SECRET_LENGTH,
                "MC_BINDING_TOKEN_SECRET is too short"
            );
            return Err(format!(
                "MC_BINDING_TOKEN_SECRET must be at least {MIN_SECRET_LENGTH} bytes, got {}",
                secret_bytes.len()
            )
            .into());
        }

        SecretBox::new(Box::new(secret_bytes))
    };

    // Per-(meeting, handler) `policy_generation` registry (ADR-0036 §8).
    // Shared between the WebTransport push path (which takes generations) and
    // the controller actor (which evicts them on meeting teardown).
    let policy_generations = Arc::new(mc_service::media_routing::PolicyGenerations::new());

    // KEK rotation lifecycle (ADR-0036 §4 Rotation). Built ONCE from the W
    // validated at load, then shared: every meeting actor's debounce, the
    // `kek_rotation_debounce_seconds` wire field and the published window gauge
    // read W from this one value, so they cannot drift.
    let kek_lifecycle = Arc::new(config.kek_lifecycle());
    kek_lifecycle.publish_config_gauges();

    // Meeting-ended notifications to GC (ADR-0010 §3). The controller queues
    // one per meeting that ended AND whose handlers are released; the GC task
    // below drains them once the GC client exists. See `MeetingEndedQueue`.
    let (meeting_ended_queue, meeting_ended_rx) =
        MeetingEndedQueue::with_capacity(MEETING_ENDED_QUEUE_CAPACITY);
    let controller_handle = Arc::new(MeetingControllerActorHandle::with_meeting_ended_notifier(
        config.mc_id.clone(),
        Arc::clone(&actor_metrics),
        Arc::clone(&controller_metrics),
        master_secret,
        Arc::clone(&policy_generations),
        Arc::clone(&kek_lifecycle),
        Arc::new(meeting_ended_queue),
    ));
    info!("Actor system initialized");

    // Create shutdown token as child of controller's token
    // This ensures all tasks are cancelled when the controller shuts down
    let shutdown_token = controller_handle.child_token();

    // KEK fleet gauges: pending age (paged on) and sender-id consumption, each a
    // MAXIMUM across live meetings. Sampled on a timer from `Instant`s the
    // actors wrote — not pushed by the actors — so a WEDGED actor still shows a
    // growing age, which is the case `MCKekRotationOverdue` exists for. The
    // interval's first tick is immediate, so both gauges read 0 from boot rather
    // than "No data".
    let kek_gauge_token = shutdown_token.child_token();
    let kek_gauge_lifecycle = Arc::clone(&kek_lifecycle);
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(
            mc_service::media_admission::rotation::KEK_GAUGE_REFRESH_INTERVAL,
        );
        loop {
            tokio::select! {
                () = kek_gauge_token.cancelled() => break,
                _ = ticker.tick() => kek_gauge_lifecycle.publish_fleet_gauges().await,
            }
        }
    });

    // Start health HTTP server (MUST succeed - fail startup if it doesn't)
    // This provides liveness/readiness probes and Prometheus /metrics endpoint
    let health_addr: SocketAddr = config.health_bind_address.parse().map_err(|e| {
        error!(error = %e, addr = %config.health_bind_address, "Invalid health bind address");
        format!("Invalid health bind address: {e}")
    })?;

    let health_router = health_router(Arc::clone(&health_state));

    // Add /metrics endpoint served by Prometheus exporter
    let metrics_router = Router::new().route(
        "/metrics",
        axum::routing::get(move || {
            let handle = prometheus_handle.clone();
            async move { handle.render() }
        }),
    );

    let app = health_router.merge(metrics_router);

    // Bind listener BEFORE spawning to fail fast on bind errors
    let listener = tokio::net::TcpListener::bind(health_addr)
        .await
        .map_err(|e| {
            error!(error = %e, addr = %health_addr, "Failed to bind health server");
            format!("Failed to bind health server to {health_addr}: {e}")
        })?;
    info!(addr = %health_addr, "Health server bound successfully");

    // Spawn health server task
    let health_shutdown_token = shutdown_token.child_token();
    tokio::spawn(async move {
        info!(addr = %health_addr, "Health server starting");
        let server = axum::serve(listener, app).with_graceful_shutdown(async move {
            health_shutdown_token.cancelled().await;
            info!("Health server shutting down");
        });
        if let Err(e) = server.await {
            error!(error = %e, "Health server failed");
        }
    });
    info!(addr = %health_addr, "Health server started");

    // Start gRPC server BEFORE GC registration (correct ordering)
    // This prevents race condition where GC tries to call MC before server is ready
    let grpc_addr = config.grpc_bind_address.parse().map_err(|e| {
        error!(error = %e, addr = %config.grpc_bind_address, "Invalid gRPC bind address");
        e
    })?;

    let mc_assignment_service = McAssignmentService::new(
        Arc::clone(&controller_handle),
        Arc::clone(&redis_client),
        config.mc_id.clone(),
        config.max_meetings,
        config.max_participants,
    );

    // Create MediaCoordinationService for MH→MC notifications (R-15)
    // Participant connectivity is recorded by the meeting actors (one home);
    // this service only validates and routes notifications to them.
    let media_coord_service = McMediaCoordinationService::new(Arc::clone(&controller_handle));

    // Create JWKS-based auth layer for gRPC service token validation (R-22)
    // Applied at the server level: validates JWT signature + expiry for ALL
    // incoming gRPC calls (both GC→MC and MH→MC). Scope checks are performed
    // by individual handlers as needed. Reuses the existing JwksClient.
    #[expect(
        clippy::cast_possible_wrap,
        reason = "clock_skew_seconds bounded to <=600, safe u64->i64"
    )]
    let mc_auth_layer =
        McAuthLayer::new(Arc::clone(&jwks_client), config.clock_skew_seconds as i64);

    let grpc_shutdown_token = shutdown_token.child_token();
    let grpc_server = tonic::transport::Server::builder()
        // R-56: `TraceLayer::new_for_grpc()` creates the per-request tracing span
        // that `server_interceptor()`'s `set_parent` attaches the inbound W3C
        // context to — WITHOUT it, `set_parent` runs before any span exists at
        // the dispatch point and is a silent no-op (every inbound call becomes a
        // fresh root; verified by the inbound-continuity integration test).
        //
        // It MUST be the FIRST/outermost `.layer()`: `McAuthService`
        // (grpc/auth_interceptor.rs) is bound to `http::Response<BoxBody>`
        // exactly, so wrapping it in `TraceLayer` (which changes the body type)
        // fails to compile (E0271). Auth must wrap `Routes` (→ BoxBody) directly;
        // TraceLayer wraps auth's output. Identical constraint + ordering to
        // GC #26 (single builder + BoxBody-bound auth layer). Zero new deps —
        // tower-http's "trace" feature is already enabled.
        .layer(tower_http::trace::TraceLayer::new_for_grpc())
        .layer(mc_auth_layer)
        .add_service(MeetingControllerServiceServer::with_interceptor(
            mc_assignment_service,
            common::observability::otel_grpc::server_interceptor(),
        ))
        .add_service(MediaCoordinationServiceServer::with_interceptor(
            media_coord_service,
            common::observability::otel_grpc::server_interceptor(),
        ))
        .serve_with_shutdown(grpc_addr, async move {
            grpc_shutdown_token.cancelled().await;
            info!("gRPC server shutting down");
        });

    // Spawn gRPC server task
    tokio::spawn(async move {
        info!(addr = %grpc_addr, "gRPC server starting");
        if let Err(e) = grpc_server.await {
            error!(error = %e, "gRPC server failed");
        }
    });
    info!(addr = %grpc_addr, "gRPC server started");

    // Create GcClient with TokenReceiver and spawn unified GC task (Phase 6c, ADR-0010)
    // This task owns gc_client directly (no Arc needed)
    info!("Connecting to Global Controller...");
    let gc_client = GcClient::new(config.gc_grpc_url.clone(), token_rx.clone(), config.clone())
        .await
        .map_err(|e| {
            error!(error = %e, "Failed to connect to GC");
            e
        })?;
    info!("Connected to Global Controller");

    // The GC client is shared by the heartbeat task and the meeting-ended drain.
    let gc_client = Arc::new(gc_client);
    let notify_client = Arc::clone(&gc_client);
    // NOT a child of the shutdown token, deliberately. A meeting whose teardown
    // completes inside the shutdown release window calls `meeting_ended` at that
    // moment, so its notification is produced AFTER the shutdown token fires. A
    // child token would end this drain at t=0 and guarantee the loss of exactly
    // those notifications — a stale GC assignment row, i.e. the failure the
    // revive fix exists to prevent, reached through the release window itself.
    // Cancelled below, after the releases have settled.
    let notify_token = tokio_util::sync::CancellationToken::new();
    let notify_drain_token = notify_token.clone();
    let notify_drain = tokio::spawn(async move {
        mc_service::grpc::gc_client::drain_meeting_ended(
            notify_client,
            meeting_ended_rx,
            notify_drain_token,
        )
        .await;
    });

    // Spawn unified GC task (registration + dual heartbeats)
    let gc_task_token = shutdown_token.child_token();
    let gc_task_metrics = Arc::clone(&controller_metrics);
    let gc_task_health = Arc::clone(&health_state);
    tokio::spawn(async move {
        run_gc_task(gc_client, gc_task_metrics, gc_task_health, gc_task_token).await;
    });
    info!("GC task started");

    info!("Meeting Controller Phase 6c: GC integration complete");

    // Create MH registration client for async RegisterMeeting RPCs (R-12)
    let mh_client: Arc<dyn MhRegistrationClient> = Arc::new(MhClient::new(token_rx.clone()));

    // Start WebTransport server (R-5: HTTP/3 over QUIC with TLS 1.3 on port 4433)
    let wt_server = WebTransportServer::new(
        config.webtransport_bind_address.clone(),
        config.tls_cert_path.clone(),
        config.tls_key_path.clone(),
        Arc::clone(&controller_handle),
        jwt_validator,
        Arc::clone(&redis_client) as Arc<dyn mc_service::redis::MhAssignmentStore>,
        mh_client,
        Arc::clone(&policy_generations),
        config.mc_id.clone(),
        config.grpc_advertise_address.clone(),
        config.client_media_config(),
        config.max_participants as usize,
        std::time::Duration::from_secs(config.quic_max_idle_timeout_seconds),
        shutdown_token.child_token(),
    );

    // Fail-fast: load TLS + bind endpoint BEFORE spawning the accept loop.
    // If certs are missing/corrupt or the port is in use, crash startup immediately
    // rather than running an MC that can never accept WebTransport connections.
    let wt_endpoint = wt_server.bind().await.map_err(|e| {
        error!(error = %e, "WebTransport server failed to bind");
        Box::<dyn std::error::Error>::from(e.to_string())
    })?;
    info!(
        addr = %config.webtransport_bind_address,
        "WebTransport endpoint bound"
    );

    tokio::spawn(async move {
        wt_server.accept_loop(wt_endpoint).await;
        info!("WebTransport accept loop stopped");
    });

    // Wait for shutdown signal
    info!("Meeting Controller running - press Ctrl+C to shutdown");
    shutdown_signal().await;

    // Trigger graceful shutdown via cancellation token
    // This propagates to all child tokens (GC task, gRPC server, health server)
    info!("Shutdown signal received, initiating graceful shutdown...");

    // Mark as not ready immediately so k8s stops sending traffic
    health_state.set_not_ready();

    shutdown_token.cancel();

    // Give tasks time to shut down
    tokio::time::sleep(Duration::from_secs(
        mc_service::media_routing::teardown::SHUTDOWN_PRE_DRAIN_SECONDS,
    ))
    .await;

    // Drain the actor system, then let the meetings' MH releases finish — both
    // under ONE deadline derived from the pod's termination grace, so this can
    // never hold the pod into SIGKILL. `shutdown` returns only once every
    // meeting actor has exited, i.e. once every teardown has been handed off
    // and counted; settling before that could observe zero and exit early.
    let release_budget = mc_service::media_routing::teardown::SHUTDOWN_RELEASE_BUDGET;
    let release_deadline = tokio::time::Instant::now() + release_budget;
    match tokio::time::timeout_at(release_deadline, controller_handle.shutdown(release_budget))
        .await
    {
        Ok(Ok(())) => {}
        Ok(Err(e)) => warn!(error = %e, "Actor system shutdown error"),
        Err(_) => warn!(
            release_budget_seconds = release_budget.as_secs(),
            "Actor system drain did not finish inside the shutdown release budget"
        ),
    }
    let teardowns = controller_handle.teardowns();
    let cut_off = teardowns.settle_until(release_deadline).await;
    if cut_off == 0 {
        info!("Meeting teardowns settled before exit");
    } else {
        // Logged, not counted: a counter incremented this close to exit is
        // never scraped. Each cut-off meeting keeps its MH registration until
        // that handler restarts (docs/TODO.md, "A meeting whose MC never sends
        // `EndMeeting` is never reclaimed").
        error!(
            teardowns_cut_off = cut_off,
            release_budget_seconds = release_budget.as_secs(),
            "Meeting teardowns still running at the shutdown deadline were cut off; \
             their handlers keep those registrations until they restart"
        );
    }

    // Only now: every release that will complete has completed, so every
    // meeting-ended notification that will be produced has been queued. The
    // drain flushes what is queued within its own reserved slice of the pod
    // grace; awaiting it here is what makes the flush actually run rather than
    // being dropped with the runtime, and BOTH sides are bounded (the drain by
    // its flush deadline, this await by the same budget) so neither can hold
    // the pod into SIGKILL.
    notify_token.cancel();
    if tokio::time::timeout(
        mc_service::media_routing::teardown::SHUTDOWN_NOTIFY_FLUSH_BUDGET,
        notify_drain,
    )
    .await
    .is_err()
    {
        warn!(
            notify_flush_budget_seconds =
                mc_service::media_routing::teardown::SHUTDOWN_NOTIFY_FLUSH_BUDGET.as_secs(),
            "GC meeting-ended flush did not finish inside its budget; any notification still \
             queued is reported on its own ERROR line"
        );
    }

    // Abort TokenManager background task (ADR-0010)
    info!("Stopping TokenManager...");
    token_task_handle.abort();

    info!("Meeting Controller shutdown complete");
    Ok(())
}

/// Unified GC task: registration + dual heartbeat loop.
///
/// Shares `gc_client` with the meeting-ended drain.
/// It never exits on GC connectivity issues - keeps retrying to protect active meetings.
///
/// Operational model:
/// - Initial registration: Retry forever until success (with exponential backoff)
/// - Dual heartbeats: Fast (10s) + comprehensive (30s) in single select loop
/// - Re-registration: Detect `NOT_FOUND` from heartbeat, automatically re-register
/// - Never exit: Protects active meetings during GC outages/restarts
async fn run_gc_task(
    gc_client: Arc<GcClient>,
    metrics: Arc<ControllerMetrics>,
    health_state: Arc<HealthState>,
    cancel_token: CancellationToken,
) {
    info!("GC task: Starting initial registration");

    // Initial registration (retry forever, never exit)
    loop {
        tokio::select! {
            () = cancel_token.cancelled() => {
                info!("GC task: Cancelled before registration completed");
                return;
            }
            result = gc_client.register() => {
                match result {
                    Ok(()) => {
                        info!("GC task: Initial registration successful");
                        // Mark as ready now that we're registered with GC
                        health_state.set_ready();
                        break; // Proceed to heartbeat loop
                    }
                    Err(e) => {
                        // Log but never exit - keep retrying
                        // GC may be temporarily unavailable during rolling updates
                        warn!(error = %e, "GC task: Initial registration failed, will retry");
                        tokio::time::sleep(Duration::from_secs(5)).await;
                    }
                }
            }
        }
    }

    // Dual heartbeat loop (fast + comprehensive in one select)
    let fast_interval = Duration::from_millis(gc_client.fast_heartbeat_interval_ms());
    let comprehensive_interval =
        Duration::from_millis(gc_client.comprehensive_heartbeat_interval_ms());

    let mut fast_ticker = tokio::time::interval(fast_interval);
    fast_ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    let mut comprehensive_ticker = tokio::time::interval(comprehensive_interval);
    comprehensive_ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    info!(
        fast_interval_ms = fast_interval.as_millis(),
        comprehensive_interval_ms = comprehensive_interval.as_millis(),
        "GC task: Entering dual heartbeat loop"
    );

    loop {
        tokio::select! {
            () = cancel_token.cancelled() => {
                info!("GC task: Shutting down");
                break;
            }
            _ = fast_ticker.tick() => {
                let snapshot = metrics.snapshot();

                if let Err(e) = gc_client
                    .fast_heartbeat(snapshot.meetings, snapshot.participants, HealthStatus::Healthy)
                    .await
                {
                    handle_heartbeat_error(&gc_client, e).await;
                }
            }
            _ = comprehensive_ticker.tick() => {
                let snapshot = metrics.snapshot();
                let sys_info = gather_system_info();

                // CPU and memory are 0-100, no precision loss in f32 range
                #[allow(clippy::cast_precision_loss)]
                let cpu = sys_info.cpu_percent as f32;
                #[allow(clippy::cast_precision_loss)]
                let memory = sys_info.memory_percent as f32;

                if let Err(e) = gc_client
                    .comprehensive_heartbeat(
                        snapshot.meetings,
                        snapshot.participants,
                        HealthStatus::Healthy,
                        cpu,
                        memory,
                    )
                    .await
                {
                    handle_heartbeat_error(&gc_client, e).await;
                }
            }
        }
    }

    info!("GC task: Stopped");
}

/// Handle heartbeat errors, including re-registration on `NOT_FOUND`.
///
/// Never exits - logs error and attempts re-registration if needed.
async fn handle_heartbeat_error(gc_client: &GcClient, error: McError) {
    match error {
        McError::NotRegistered => {
            // GC doesn't recognize this MC (e.g., after GC restart)
            // Attempt re-registration (single attempt, task loop will retry)
            warn!("Heartbeat failed: MC not registered with GC, attempting re-registration");

            if let Err(e) = gc_client.attempt_reregistration().await {
                warn!(error = %e, "Re-registration failed, will retry on next heartbeat");
            } else {
                info!("Re-registration successful");
            }
        }
        other => {
            // Other errors (network, timeout, etc.) - log and continue
            warn!(error = %other, "Heartbeat failed");
        }
    }
}

/// Wait for shutdown signal (Ctrl+C or SIGTERM).
///
/// # Panics
///
/// Panics if signal handlers cannot be installed. This is acceptable because
/// without signal handlers, we cannot gracefully shut down the service.
async fn shutdown_signal() {
    let ctrl_c = async {
        #[expect(
            clippy::expect_used,
            reason = "Signal handler installation is critical - panic is appropriate if it fails"
        )]
        signal::ctrl_c()
            .await
            .expect("Failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        #[expect(
            clippy::expect_used,
            reason = "Signal handler installation is critical - panic is appropriate if it fails"
        )]
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("Failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => {}
        () = terminate => {}
    }
}
