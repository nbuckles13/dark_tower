# DRY Reviewer Navigation

## Architecture & Design
- Blocking vs tech-debt classification -> ADR-0019 (`docs/decisions/adr-0019-dry-reviewer.md`)
- Fix-or-defer review model -> ADR-0024 (`docs/decisions/adr-0024-agent-teams-workflow.md`)

## JWT Validation (Common + Thin Wrappers)
- Common JWT code (all shared logic) -> `crates/common/src/jwt.rs`
- Internal token request types (GC->AC contract) -> `crates/common/src/meeting_token.rs`
- GC thin wrapper -> `crates/gc-service/src/auth/jwt.rs` (ServiceClaims, UserClaims)
- MC thin wrapper -> `crates/mc-service/src/auth/mod.rs` (MeetingTokenClaims, GuestTokenClaims)
- MH thin wrapper -> `crates/mh-service/src/auth/mod.rs` (MeetingTokenClaims only, no guest tokens)

## Per-Service Observability (Metrics & Dashboards)
- AC/GC/MC/MH metrics -> `crates/*/src/observability/metrics.rs` (per-service, not duplication)
- MC/GC join alert rules -> `infra/docker/prometheus/rules/{mc,gc}-alerts.yaml`
- Dashboard metric presentation -> ADR-0029
- Grafana dashboards + configMapGenerator -> `infra/grafana/dashboards/`, `infra/kubernetes/observability/grafana/`

## Integration Test Coverage
- MC join flow (11 tests) -> `crates/mc-service/tests/join_tests.rs`
- GC join/guest/settings -> `crates/gc-service/tests/meeting_tests.rs`
- MH GC integration -> `crates/mh-service/tests/gc_integration.rs`
- Shared fixtures -> `crates/mc-test-utils/src/jwt_test.rs`, `crates/gc-test-utils/src/server_harness.rs`

## Per-Service Config Parsing
- AC/GC/MC/MH config -> `crates/*/src/config.rs:Config::from_vars()` (per-service, not duplication)
- Advertise addresses (MC + MH) -> POD_IP (gRPC), per-instance ConfigMap (WebTransport), setup.sh patching
- StatefulSet ordinal -> `crates/common/src/config.rs:parse_statefulset_ordinal()` (shared)
- Extraction candidate: `generate_instance_id(prefix)` -> duplicated in GC + MC + MH config

## gRPC Auth (Cross-Service)
- MC auth interceptor (sync, structural only) -> `crates/mc-service/src/grpc/auth_interceptor.rs:McAuthInterceptor`
- MH auth layer (async, JWKS + scope check) -> `crates/mh-service/src/grpc/auth_interceptor.rs:MhAuthLayer`
- MH legacy interceptor (structural, kept for compat) -> same file, `MhAuthInterceptor`
- Shared constant -> `common::jwt::MAX_JWT_SIZE_BYTES`

## WebTransport Servers (MC + MH)
- MC server + connection (signaling) -> `crates/mc-service/src/webtransport/server.rs`, `connection.rs`
- MH server + connection (media, provisional accept) -> `crates/mh-service/src/webtransport/server.rs`, `connection.rs`
- MH session tracking -> `crates/mh-service/src/session/mod.rs:SessionManager` (MH-specific)
- Extraction candidate: `read_framed_message` (~40 lines identical in MC + MH connection handlers)

## GC Clients (MC + MH -> GC Registration)
- MC GcClient -> `crates/mc-service/src/grpc/gc_client.rs:GcClient` (bounded retries, fast/comprehensive heartbeats)
- MH GcClient -> `crates/mh-service/src/grpc/gc_client.rs:GcClient` (unbounded retries, load reports)
- Shared patterns: channel creation, `add_auth`, backoff constants (acceptable duplication, <2 call sites)
- Extraction candidate: `add_auth` (~10 lines identical) -> extract to `common` if third service needs it
- MH gRPC stub service -> `crates/mh-service/src/grpc/mh_service.rs:MhMediaService`

## Health Endpoints (Cross-Service Consistency)
- MC health routes -> `crates/mc-service/src/observability/health.rs:health_router()`
- MH health routes -> `crates/mh-service/src/observability/health.rs:health_router()` (duplicates MC)
- GC health routes -> `crates/gc-service/src/routes/mod.rs:64-65`

## Per-Service Infrastructure (K8s, Docker, Kind)
- Kustomize bases -> `infra/services/{ac,gc,mc,mh}-service/kustomization.yaml`
- StatefulSets (AC/MC/MH) vs Deployment (GC) -> `infra/services/*/`; MC/MH per-pod WebTransport NodePorts -> `service.yaml`
- Dockerfiles -> `infra/docker/{ac,gc,mc,mh}-service/Dockerfile` (cargo-chef multi-stage)
- Kind config -> `infra/kind/kind-config.yaml{,.tmpl}`; setup/teardown -> `infra/kind/scripts/`
- Helper commands -> `crates/devloop-helper/src/commands.rs`; dev-cluster display -> `infra/devloop/dev-cluster`
- Devloop validation Layer 8 -> `.claude/skills/devloop/SKILL.md`; TLS -> `scripts/generate-dev-certs.sh`

## False Positive Boundaries
- Per-service error mapping (GcError vs McError vs MhError) -> required, not duplication; `From<JwtError>` impls in `gc/errors.rs`, `mc/errors.rs`, `mh/errors.rs`
- MC GcClient vs MH GcClient -> different RPCs, retry strategies, heartbeat models (reviewed 2026-04-01)
- AC rate limiting (DB-backed lockout) vs GC rate limiting (middleware RPM) -> different mechanisms
- common::jwt enums (2 variants) vs common::meeting_token (3 variants) -> JWT narrower (no Guest); env-tests GuestTokenRequest vs common -> different types (public vs internal)

## Tech Debt Registry
- Active duplication tech debt -> `docs/TODO.md` (Cross-Service Duplication section)

## Successful Extractions
- Common crate -> `crates/common/src/` (jwt, config, meeting_token, secret, token_manager)
- TestKeypair + JWKS mock -> `crates/mc-test-utils/src/jwt_test.rs`; JoinMeetingResponse::new() -> `gc-service/src/handlers/meetings.rs`
