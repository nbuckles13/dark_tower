# Global Controller Navigation

## Architecture & Design
- GC architecture, dual-server, MC/MH registry, load balancing -> ADR-0010
- API versioning, URL prefix conventions -> ADR-0004
- User auth, meeting access, join dependency chain -> ADR-0020
- Service-to-service auth (OAuth 2.0 client credentials) -> ADR-0003

## Code Locations
- Entrypoint (HTTP + gRPC dual-server startup) -> `crates/gc-service/src/main.rs`
- Route definitions and AppState -> `crates/gc-service/src/routes/mod.rs:build_routes()`
- Configuration (env vars, thresholds) -> `crates/gc-service/src/config.rs:Config::from_env()`
- Error types -> `crates/gc-service/src/errors.rs`
- JWT validation -> `crates/gc-service/src/auth/jwt.rs:JwtValidator::validate()`
- JWKS caching -> `crates/gc-service/src/auth/jwks.rs:JwksClient::get_key()`
- Claims extraction -> `crates/gc-service/src/auth/claims.rs`
- HTTP auth middleware -> `crates/gc-service/src/middleware/auth.rs:require_auth()`
- gRPC auth layer (Tower) -> `crates/gc-service/src/grpc/auth_layer.rs:GrpcAuthLayer`
- Meeting handlers (join, guest, settings) -> `crates/gc-service/src/handlers/meetings.rs`
- MC gRPC service (register, heartbeat) -> `crates/gc-service/src/grpc/mc_service.rs:McService`
- MH gRPC service (register, load report) -> `crates/gc-service/src/grpc/mh_service.rs:MhService`
- MC assignment + load balancing -> `crates/gc-service/src/services/mc_assignment.rs:McAssignmentService`
- MH selection (primary + backup AZ) -> `crates/gc-service/src/services/mh_selection.rs:MhSelectionService`
- MC gRPC client (assign_meeting RPC) -> `crates/gc-service/src/services/mc_client.rs:McClientTrait`
- AC HTTP client (meeting/guest tokens) -> `crates/gc-service/src/services/ac_client.rs:AcClient`
- MC repository (register, heartbeat, staleness) -> `crates/gc-service/src/repositories/meeting_controllers.rs`
- MH repository -> `crates/gc-service/src/repositories/media_handlers.rs`
- Assignment repository (weighted select, atomic assign) -> `crates/gc-service/src/repositories/meeting_assignments.rs`
- Generic health checker loop -> `crates/gc-service/src/tasks/generic_health_checker.rs`
- Assignment cleanup (soft/hard delete) -> `crates/gc-service/src/tasks/assignment_cleanup.rs`
- Observability metrics -> `crates/gc-service/src/observability/metrics.rs`
- Grafana dashboard -> `infra/grafana/dashboards/gc-overview.json`

## Integration Seams
- GC <-> AC (OAuth token refresh) -> `crates/common/src/token_manager.rs:TokenReceiver`
- GC <-> AC (meeting/guest token issuance) -> `crates/gc-service/src/services/ac_client.rs`
- GC <-> MC (gRPC registration + heartbeat) -> `crates/gc-service/src/grpc/mc_service.rs`
- GC <-> MC (gRPC assignment RPC) -> `crates/gc-service/src/services/mc_client.rs`
- GC <-> MH (gRPC registration + load report) -> `crates/gc-service/src/grpc/mh_service.rs`
- GC <-> Client (HTTP API /api/v1/*) -> `crates/gc-service/src/routes/mod.rs`
- env-tests GC client fixture -> `crates/env-tests/src/fixtures/gc_client.rs`
