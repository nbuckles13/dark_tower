//! Media Handler
//!
//! SFU (Selective Forwarding Unit) for real-time media routing.
//!
//! # Servers
//!
//! The Media Handler runs multiple servers:
//! - gRPC server for MC→MH communication (default: 0.0.0.0:50053)
//! - HTTP server for health endpoints (default: 0.0.0.0:8083)
//! - WebTransport server for client media (default: 0.0.0.0:4434) — stub
//!
//! # Startup Flow (ADR-0010)
//!
//! 1. Load configuration from environment
//! 2. Initialize Prometheus metrics recorder (ADR-0011)
//! 3. Spawn `TokenManager` for OAuth token acquisition from AC
//! 4. Start health HTTP server (liveness, readiness, metrics)
//! 5. Start gRPC server for MC→MH communication
//! 6. Create `GcClient` and spawn GC task (registration + load reports)
//! 7. Wait for shutdown signal

#![warn(clippy::pedantic)]
#![allow(clippy::too_many_lines)]

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use common::jwt::JwksClient;
use common::token_manager::{spawn_token_manager, TokenManagerConfig};
use mh_service::auth::MhJwtValidator;
use mh_service::config::Config;
use mh_service::errors::MhError;
use mh_service::grpc::{GcClient, McClient, MhAuthLayer, MhMediaService, SpanLayer};
use mh_service::observability::{health_router, HealthState};
use mh_service::session::SessionManagerHandle;
use mh_service::webtransport::WebTransportServer;
use proto_gen::dark_tower::internal::v1::media_handler_service_server::MediaHandlerServiceServer;
use tokio::signal;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

/// Default timeout for initial token acquisition.
const TOKEN_ACQUISITION_TIMEOUT: Duration = Duration::from_secs(30);

/// Maximum accepted encoded size of an inbound MC→MH gRPC message.
///
/// 4 MiB, matching tonic's own default — stated rather than inherited. See the
/// call site for why this is the outermost bound on the control plane.
const MAX_GRPC_DECODING_BYTES: usize = 4 * 1024 * 1024;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load configuration BEFORE the tracing subscriber: the R-55 OTel layer
    // needs the endpoint/sample-rate from config, and the subscriber can only
    // be `.init()`'d once. A config-load failure here has no subscriber to log
    // through yet — it propagates as a non-zero exit via the default Debug-to-
    // stderr dump, same as AC's exemplar; nothing is lost because no spans/logs
    // have been emitted yet.
    let config = Config::from_env()?;

    // R-55: initialize the OpenTelemetry SDK only when explicitly enabled
    // (OTEL_ENABLED=true). `init_otel` is async, eagerly probes the collector,
    // and fails hard at init on an unreachable/misconfigured endpoint so the
    // pod fails K8s readiness instead of silently dropping spans. When OTel is
    // disabled, `otel_config()` returns `None`, `init_otel` is never called (no
    // probe), and no OTel layer is composed.
    let otel = match config.otel_config() {
        Some(otel_cfg) => Some(
            common::observability::otel::init_otel(
                "media-handler",
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
    // composes onto the bare registry first; the default-off path adds zero
    // layers. JSON format enables robust parsing in Promtail without brittle regex.
    tracing_subscriber::registry()
        .with(otel_layer)
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "mh_service=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer().json())
        .init();

    info!("Starting Media Handler");

    info!(
        region = %config.region,
        handler_id = %config.handler_id,
        grpc_bind_address = %config.grpc_bind_address,
        health_bind_address = %config.health_bind_address,
        webtransport_bind_address = %config.webtransport_bind_address,
        grpc_advertise_address = %config.grpc_advertise_address,
        webtransport_advertise_address = %config.webtransport_advertise_address,
        max_streams = config.max_streams,
        max_connections = config.max_connections,
        register_meeting_timeout_seconds = config.register_meeting_timeout_seconds,
        // ADR-0036 §1 QUIC transport parameters and the derived drain window.
        //
        // Layer-qualified `transport_` prefix on purpose: without it this line
        // would carry three unrelated `max_`-prefixed fields — `max_streams`
        // (advertised to GC, enforced only at GC placement), `max_connections`
        // (enforced by MH at accept), and the QUIC uni-stream bound (enforced
        // by quinn) — whose names suggest a kinship they do not have. The
        // prefix states which layer enforces the value.
        //
        // BOTH links of the frames->bytes conversion are printed. Logging only
        // frames hides the conversion at the exact moment an operator is
        // debugging it; logging bytes under a `_frames` name is the unit lie
        // this task exists to remove. `..._ms` is the figure ADR-0036 §1
        // actually reasons with (its "93 seconds of queued audio").
        //
        // Provenance is NOT visible here and cannot be: some of these are
        // ConfigMap keys and some are compile-time constants. `config.rs`
        // groups them so the distinction is legible in one place.
        transport_max_concurrent_uni_streams = config.quic_transport.max_concurrent_uni_streams,
        transport_datagram_send_buffer_audio_frames =
            config.quic_transport.datagram_send_buffer_audio_frames,
        transport_datagram_send_buffer_bytes = config.quic_transport.datagram_send_buffer_bytes,
        transport_datagram_send_buffer_ms = config.quic_transport.datagram_send_buffer_ms(),
        transport_datagram_receive_buffer_bytes = mh_service::config::DATAGRAM_RECEIVE_BUFFER_BYTES,
        // The negotiated wire limit. quinn derives it from the receive buffer,
        // so an operator debugging datagrams too large to send would otherwise
        // have to read quinn's source to learn it.
        transport_max_datagram_frame_size_bytes =
            config.quic_transport.max_datagram_frame_size_bytes(),
        transport_receive_window_bytes = mh_service::config::CONNECTION_RECEIVE_WINDOW_BYTES,
        transport_stream_receive_window_bytes = mh_service::config::STREAM_RECEIVE_WINDOW_BYTES,
        transport_keepalive_interval_ms = config.quic_transport.keepalive_interval_ms,
        transport_max_idle_timeout_ms = mh_service::config::MAX_IDLE_TIMEOUT_SECONDS * 1_000,
        // BOTH conversion factors, so `frames x bytes_factor = bytes` AND
        // `frames x ms_factor = ms` are each checkable from this one line
        // rather than from source. The ms link matters at least as much as the
        // bytes link: `transport_datagram_send_buffer_ms` is the field an
        // operator actually reasons with, since queued-audio latency is the
        // quantity ADR-0036 §1 frames its whole complaint around.
        nominal_audio_frame_bytes = mh_service::config::NOMINAL_AUDIO_FRAME_BYTES,
        audio_frame_duration_ms = mh_service::config::AUDIO_FRAME_DURATION_MS,
        egress_queue_frames = mh_service::config::EGRESS_QUEUE_FRAMES,
        // Drain chain: seconds end to end, so nothing here is a unit conversion.
        // `drain_window_source` names which arm of the min() won — the only way
        // to tell "2s because that is the settle target" from "1s because the
        // grace was set to 6" without reading source.
        termination_grace_seconds = config.termination_grace_seconds,
        shutdown_settle_target_seconds = mh_service::config::SHUTDOWN_SETTLE_TARGET_SECONDS,
        shutdown_margin_seconds = mh_service::config::SHUTDOWN_MARGIN_SECONDS,
        drain_window_seconds = config.drain_window.as_secs(),
        drain_window_source = config.drain_window_source.as_str(),
        // ADR-0036 §8 policy bounds. Logged because all four are
        // optional-with-default: without this line an operator cannot tell a
        // deliberately-configured value from a default nobody chose, which is
        // the failure mode `MH_MAX_CONNECTIONS`'s 10,000 default already had.
        max_egress_streams_per_meeting = config.policy_limits.max_egress_streams_per_meeting,
        max_candidate_sources_per_egress = config.policy_limits.max_candidate_sources_per_egress,
        max_total_egress_edges = config.policy_limits.max_total_egress_edges,
        policy_apply_timeout_ms = config.policy_limits.policy_apply_timeout_ms,
        "Configuration loaded successfully"
    );

    // Initialize Prometheus metrics recorder (ADR-0011)
    // This must happen before any metrics are recorded
    info!("Initializing Prometheus metrics recorder...");
    let prometheus_handle =
        mh_service::observability::metrics::init_metrics_recorder().map_err(|e| {
            error!(error = %e, "Failed to install Prometheus metrics recorder");
            e
        })?;
    info!("Prometheus metrics recorder initialized");

    // Initialize health state
    let health_state = Arc::new(HealthState::new());

    // Create shutdown token
    let shutdown_token = CancellationToken::new();

    // Spawn TokenManager for OAuth token acquisition (ADR-0003)
    info!(
        ac_endpoint = %config.ac_endpoint,
        client_id = %config.client_id,
        "Spawning TokenManager for AC authentication..."
    );

    let token_config = TokenManagerConfig::from_url(
        config.ac_endpoint.clone(),
        config.client_id.clone(),
        config.client_secret.clone(),
    )
    .map_err(|e| {
        error!(error = %e, "Failed to create TokenManager config");
        MhError::TokenAcquisition(format!("TokenManager config error: {e}"))
    })?
    .with_on_refresh(Arc::new(|event| {
        mh_service::observability::metrics::record_token_refresh_metrics(&event);
    }));

    let (token_task_handle, token_rx) =
        tokio::time::timeout(TOKEN_ACQUISITION_TIMEOUT, spawn_token_manager(token_config))
            .await
            .map_err(|_| {
                error!(
                    timeout_secs = TOKEN_ACQUISITION_TIMEOUT.as_secs(),
                    "Token acquisition timed out - AC may be unreachable"
                );
                MhError::TokenAcquisitionTimeout
            })?
            .map_err(|e| {
                error!(error = %e, "Failed to acquire initial token from AC");
                MhError::TokenAcquisition(format!("Initial token acquisition failed: {e}"))
            })?;

    info!("TokenManager spawned successfully, initial token acquired");

    // Initialize JWKS client for JWT validation (meeting tokens + service tokens)
    info!(
        ac_jwks_url = %config.ac_jwks_url,
        "Initializing JWKS client..."
    );
    let jwks_client = Arc::new(JwksClient::new(config.ac_jwks_url.clone()).map_err(|e| {
        error!(error = %e, "Failed to create JWKS client");
        MhError::Config(format!("JWKS client creation failed: {e}"))
    })?);

    // Create MH JWT validator for meeting tokens (WebTransport connections)
    let jwt_validator = Arc::new(MhJwtValidator::new(Arc::clone(&jwks_client), 300));
    info!("JWKS client and JWT validator initialized");

    // Create session manager actor for meeting registration and connection tracking
    let session_manager = SessionManagerHandle::new();
    info!("Session manager actor spawned");

    // Create MC notification client for MH→MC participant notifications (R-16/R-17)
    let mc_client = Arc::new(McClient::new(token_rx.clone()));
    info!("MC notification client created");

    // Start health HTTP server (MUST succeed - fail startup if it doesn't)
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
    // This prevents race condition where MC tries to call MH before server is ready
    let grpc_addr: SocketAddr = config.grpc_bind_address.parse().map_err(|e| {
        error!(error = %e, addr = %config.grpc_bind_address, "Invalid gRPC bind address");
        format!("Invalid gRPC bind address: {e}")
    })?;

    // Sampled ONCE, here, and carried unchanged for the process's life
    // (ADR-0036 §8 restart detection). Deliberately at the call site rather
    // than behind a lazily-initialised static, so "once per process" is
    // visible where the value is passed in: a per-call `now()` makes MC's
    // restart detector fire constantly, and a pod-name or config derivation
    // makes it never fire for the crash-restart it exists to catch.
    let process_start_epoch_ms = mh_service::process::sample_process_start_epoch_ms();
    let mh_media_service = MhMediaService::new(
        session_manager.clone(),
        config.handler_id.clone(),
        process_start_epoch_ms,
        config.policy_limits,
    );
    let auth_layer = MhAuthLayer::new(Arc::clone(&jwks_client), 300);

    // R-56: extract inbound W3C trace context (traceparent/tracestate) from
    // MC's request metadata and attach it as the parent of the handler span.
    // Independent of `auth_layer` above — that reads `authorization`, this
    // reads only the two W3C headers (common::observability::otel_grpc).
    //
    // `SpanLayer` is REQUIRED for the interceptor's `set_parent` call to have
    // any effect: it supplies the ambient tracing span the interceptor
    // attaches to and keeps active through the handler's own `#[instrument]`
    // span (see `grpc::span_layer` module docs — without it, the extracted
    // context is silently dropped and every inbound call becomes a fresh
    // root trace, verified empirically).
    let grpc_shutdown_token = shutdown_token.child_token();
    let grpc_server = tonic::transport::Server::builder()
        .layer(SpanLayer)
        .layer(auth_layer)
        .add_service(tonic::service::interceptor::InterceptedService::new(
            // Explicit, not implicit. `RegisterMeetingRequest` grew two
            // unbounded repeated fields with the ADR-0036 §8 reshape, so the
            // decoder is the OUTERMOST allocation bound on this path — the
            // per-meeting and per-egress bounds in `PolicyLimits` only apply
            // once a message has already been decoded into memory. tonic's
            // default happens to be 4 MiB; stating it means a future default
            // change cannot silently widen MH's control-plane allocation
            // surface.
            //
            // Built explicitly rather than via `with_interceptor` because the
            // size limit belongs on the generated server, and
            // `InterceptedService` does not forward it.
            MediaHandlerServiceServer::new(mh_media_service)
                .max_decoding_message_size(MAX_GRPC_DECODING_BYTES),
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

    // Media forward-path setup (ADR-0036 §11): every metric handle the hot
    // path uses is resolved ONCE, here, before any connection exists — so no
    // registry lookup can occur per frame. This is the sibling half of the
    // layout constraint; `crate::media` never resolves a handle.
    let media_setup = mh_service::media::MediaSetup {
        handles: Arc::new(mh_service::observability::metrics::resolve_media_handles()),
        latency_sample_ratio: config.media_latency_sample_ratio,
    };
    // Published ONCE, unconditionally, at process start — not per media session.
    //
    // The handle is resolved above, so the series exists from startup either
    // way; publishing it only when a session starts means it renders **0**
    // until then. `0.0` is not a neutral "unset" for this gauge: it is a legal
    // configured value inside the accepted `0.0..=1.0` range meaning "observe
    // no frames", which is the reading the ConfigMap comment describes. A
    // responder could not tell "sampling is configured off" from "no media
    // session has ever started on this pod" — opposite diagnoses. It is also
    // the detector for a ConfigMap-only edit that never reached a running pod
    // (env vars are injected at container start), and that only works if it is
    // published unconditionally.
    //
    // The one-field-two-readers property is preserved and tightened: this and
    // every `ConnectionForwarder` read `media_setup.latency_sample_ratio`, the
    // same field, and this is the ONLY publish site.
    //
    // Do not add a second one per connection. Two writers to one gauge are
    // harmless only while both compute the same number, and the moment anything
    // clamps, rounds or otherwise transforms the ratio inside
    // `ConnectionForwarder` — a locally harmless-looking change — the gauge
    // starts alternating between the startup value and the per-connection value
    // on every new connection, silently, with no failing test. A gauge that
    // flaps between two readings of "the applied ratio" is worse than either
    // reading alone and defeats the property this comment is claiming.
    media_setup
        .handles
        .publish_sample_ratio(media_setup.latency_sample_ratio);

    // Start WebTransport server BEFORE GC registration (ADR-0010 ordering)
    // This ensures MH can accept client connections before GC starts routing traffic here
    let wt_server = WebTransportServer::new(
        config.webtransport_bind_address.clone(),
        config.tls_cert_path.clone(),
        config.tls_key_path.clone(),
        Arc::clone(&jwt_validator),
        session_manager,
        Arc::clone(&mc_client),
        config.handler_id.clone(),
        Duration::from_secs(config.register_meeting_timeout_seconds),
        config.max_connections,
        config.quic_transport,
        media_setup,
        shutdown_token.child_token(),
    );

    let wt_endpoint = wt_server.bind().await.map_err(|e| {
        error!(error = %e, "Failed to bind WebTransport server");
        format!("WebTransport bind failed: {e}")
    })?;

    info!(
        addr = %config.webtransport_bind_address,
        "WebTransport server bound successfully"
    );

    tokio::spawn(async move {
        wt_server.accept_loop(wt_endpoint).await;
    });
    info!(
        addr = %config.webtransport_bind_address,
        "WebTransport accept loop started"
    );

    // Connect to Global Controller
    info!("Connecting to Global Controller...");
    let gc_client = GcClient::new(config.gc_grpc_url.clone(), token_rx.clone(), config.clone())
        .await
        .map_err(|e| {
            error!(error = %e, "Failed to connect to GC");
            e
        })?;
    info!("Connected to Global Controller");

    // Spawn GC task (registration + load report heartbeats)
    let gc_task_token = shutdown_token.child_token();
    let gc_task_health = Arc::clone(&health_state);
    tokio::spawn(async move {
        run_gc_task(gc_client, gc_task_health, gc_task_token).await;
    });
    info!("GC task started");

    info!("Media Handler running - press Ctrl+C to shutdown");

    // Wait for shutdown signal
    shutdown_signal().await;

    // Trigger graceful shutdown
    info!("Shutdown signal received, initiating graceful shutdown...");

    // Mark as not ready immediately so k8s stops sending traffic
    health_state.set_not_ready();

    shutdown_token.cancel();

    // Settle window, DERIVED at config load as
    // `min(SHUTDOWN_SETTLE_TARGET, MH_TERMINATION_GRACE_SECONDS - SHUTDOWN_MARGIN)`
    // and validated there (ADR-0036 §11: "derive rather than guard wherever two
    // values encode one relationship" — a derived value cannot drift, a guard
    // only catches drift after someone introduces it).
    //
    // No seconds literal lives here on purpose. The previous hardcoded 2s was a
    // number with no stated relationship to the 35s `terminationGracePeriodSeconds`
    // it had to fit inside; the two could be edited independently and neither
    // site mentioned the other.
    //
    // This is NOT a drain phase. ADR-0036 §11 decides that MH SHEDS media
    // sessions on restart rather than draining them, and v1 keeps it that way —
    // recovery is §8's re-assert. This window only lets in-flight teardown
    // settle. The effective value and the arm that produced it are on the
    // startup log line above.
    tokio::time::sleep(config.drain_window).await;

    // Abort TokenManager background task
    info!("Stopping TokenManager...");
    token_task_handle.abort();

    info!("Media Handler shutdown complete");
    Ok(())
}

/// Unified GC task: registration + load report heartbeat loop.
///
/// Never exits on GC connectivity issues - keeps retrying.
async fn run_gc_task(
    gc_client: GcClient,
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
                        health_state.set_ready();
                        break;
                    }
                    Err(e) => {
                        warn!(error = %e, "GC task: Initial registration failed, will retry");
                        tokio::time::sleep(Duration::from_secs(5)).await;
                    }
                }
            }
        }
    }

    // Load report heartbeat loop
    let interval_ms = gc_client.load_report_interval_ms();
    let interval = Duration::from_millis(interval_ms);

    let mut ticker = tokio::time::interval(interval);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    info!(
        interval_ms = interval_ms,
        "GC task: Entering load report heartbeat loop"
    );

    loop {
        tokio::select! {
            () = cancel_token.cancelled() => {
                info!("GC task: Shutting down");
                break;
            }
            _ = ticker.tick() => {
                if let Err(e) = gc_client.send_load_report().await {
                    match e {
                        MhError::NotRegistered => {
                            warn!("Load report failed: MH not registered with GC, attempting re-registration");
                            if let Err(re) = gc_client.attempt_reregistration().await {
                                warn!(error = %re, "Re-registration failed, will retry on next heartbeat");
                            } else {
                                info!("Re-registration successful");
                            }
                        }
                        other => {
                            warn!(error = %other, "Load report failed");
                        }
                    }
                }
            }
        }
    }

    info!("GC task: Stopped");
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
