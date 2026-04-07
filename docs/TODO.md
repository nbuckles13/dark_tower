# Technical Debt

## Inter-Service Protocol Inconsistency

- [ ] **AC internal APIs use HTTP/JSON without shared contracts**: All other inter-service communication (GC↔MC, GC↔MH, MC↔MH) uses gRPC with proto-defined contracts in `proto/internal.proto`. AC is the exception — its internal APIs (`/api/v1/auth/internal/meeting-token`, `/api/v1/auth/internal/guest-token`, `/api/v1/auth/token`) are HTTP/JSON with request/response structs defined independently on each side. This caused a production-visible bug: GC's `MeetingTokenRequest` defined `home_org_id: Option<Uuid>` while AC's expects `home_org_id: Uuid` (required), resulting in 422 errors at runtime. No compile-time check catches these mismatches. Options: (a) migrate AC internal APIs to gRPC and add to `internal.proto`, or (b) create a shared `ac-api-types` crate that both AC and GC import. The `common::token_manager` (OAuth token fetch) is already shared but the meeting/guest token request types are not. Locations: `crates/gc-service/src/services/ac_client.rs` (GC's structs), `crates/ac-service/src/models/mod.rs` (AC's structs).

## Cross-Service Duplication (DRY)

### From DRY Reviewer (Ongoing)

- [ ] **TD-9: Error response boilerplate**: `crates/ac-service/src/errors.rs`, `crates/gc-service/src/errors.rs`
- [ ] **TD-19: HTTP metrics middleware**: `crates/ac-service/src/middleware/http_metrics.rs`, `crates/gc-service/src/middleware/http_metrics.rs`
- [ ] **TD-11: Shutdown signal handlers**: `crates/*/src/main.rs`
- [ ] **Per-service observability duplication**: `crates/ac-service/src/observability/metrics.rs`, `crates/gc-service/src/observability/metrics.rs`, `crates/mc-service/src/observability/metrics.rs`
- [ ] **GC Claims struct duplicates common::jwt::ServiceClaims**: `crates/gc-service/src/auth/claims.rs` defines its own `Claims` identical to `common::jwt::ServiceClaims`. Should migrate to `pub type Claims = common::jwt::ServiceClaims;` (same pattern as AC)
- [ ] **AC private MeetingTokenClaims/GuestTokenClaims duplicate common types**: `crates/ac-service/src/handlers/internal_tokens.rs:235-264` defines private `MeetingTokenClaims`/`GuestTokenClaims` structs that are structurally near-identical to `common::jwt::MeetingTokenClaims`/`common::jwt::GuestTokenClaims`. AC should migrate to use the common types.
- [ ] **AC sign_meeting_jwt/sign_guest_jwt are identical**: `crates/ac-service/src/handlers/internal_tokens.rs:267-310` — two functions with the same body, differing only in claims type. Could be collapsed into a single generic `sign_jwt<T: Serialize>()`.
- [ ] **TestKeypair/build_pkcs8_from_seed duplication (6 locations)**: Identical Ed25519 test keypair helpers duplicated in: `crates/gc-service/src/grpc/auth_layer.rs` (tests), `crates/gc-service/tests/auth_tests.rs`, `crates/gc-service/tests/meeting_create_tests.rs`, `crates/gc-service/tests/meeting_tests.rs`, `crates/ac-test-utils/src/crypto_fixtures.rs`, and `crates/mc-service/src/auth/mod.rs` (tests). Should consolidate into a shared test-utils crate (e.g., extend `ac-test-utils` or create `common-test-utils`).
- [ ] **GC integration test fixture duplication**: `crates/gc-service/tests/participant_tests.rs`, `meeting_create_tests.rs`, `meeting_tests.rs` each re-implement org/user/meeting INSERT helpers, `TestClaims`/`TestUserClaims` structs, and `get_test_metrics_handle()`. Within `participant_tests.rs`, `create_test_fixtures_with_status` and `create_test_fixtures_with_max` duplicate the same org/user/meeting INSERT logic differing only in parameterized fields. Consider a shared `tests/common/mod.rs` or `gc-test-utils` module.
- [ ] **JoinMeetingResponse construction duplication**: `crates/gc-service/src/handlers/meetings.rs:432-442` and `:555-565` contain identical 10-line `JoinMeetingResponse { token, expires_in, meeting_id, meeting_name, mc_assignment: McAssignmentInfo { ... } }` blocks in `join_meeting` and `get_guest_token`. Low priority (2 call sites) — could extract into a `From` impl or helper.
- [ ] **setup.sh SKIP_BUILD conditional pattern (4x)**: `deploy_ac_service`, `deploy_gc_service`, `deploy_mc_service`, `deploy_mh_service` each wrap `build_image` + `load_image_to_kind` in identical `if [[ "${SKIP_BUILD}" != "true" ]]` blocks with per-service log messages. Could extract `build_and_load_if_needed <tag> <dockerfile> <service-label>`. Low priority — each instance is readable and self-contained.
- [ ] **setup.sh TLS secret creation (2x)**: `create_mc_tls_secret()` and `create_mh_tls_secret()` are near-identical — both call `generate-dev-certs.sh` then `kubectl create secret tls`. Could parameterize as `create_tls_secret <name> <cert-file> <key-file>`. Low priority (2 call sites).
- [ ] **Auth interceptor duplication (MC/MH)**: `crates/mc-service/src/grpc/auth_interceptor.rs` and `crates/mh-service/src/grpc/auth_interceptor.rs` are near-identical (~60 LOC). Extract to `common::grpc::ServiceAuthInterceptor` when JWKS validation is added (both currently do structural validation only).
- [ ] **HealthState/health_router duplication (MC/MH)**: `crates/mc-service/src/observability/health.rs` and `crates/mh-service/src/observability/health.rs` are structurally identical (HealthState + health_router + liveness/readiness handlers). Consider extracting to `common::health` module.

### From ADR-0010 Phase 4a Review (2026-01-31)

- [ ] **HealthStatus::from_proto() inconsistency**: MH uses inline match with `Pending` default, MC uses centralized method with `Unhealthy` default. Location: `crates/gc-service/src/grpc/mh_service.rs`
- [ ] **gRPC input validation duplication**: MC and MH services duplicate validation logic (~100 lines). Locations: `crates/gc-service/src/grpc/mc_service.rs`, `crates/gc-service/src/grpc/mh_service.rs`
- [ ] **Heartbeat interval constants**: Defined in 3 places with different names. Locations: `mc_service.rs`, `mh_service.rs`, `meeting-controller/gc_client.rs`

## Env-Test Self-Sufficiency

- [ ] **AC org provisioning endpoint**: Add an admin/internal API to AC for creating organizations. Env-tests should create their own test org via this endpoint instead of depending on pre-seeded data in `infra/docker/postgres/init.sql`.
- [ ] **Remove init.sql seed data**: Once the AC provisioning endpoint exists, remove the `devtest` org/user seed logic from `infra/docker/postgres/init.sql` and update env-tests (20, 21, 23) to self-provision via the API.
- [-] **Env-test portability — remove localhost/port hardcoding**: Partially resolved. `ClusterPorts::from_env()` (ADR-0030) now reads `ENV_TEST_AC_URL`, `ENV_TEST_GC_URL`, `ENV_TEST_PROMETHEUS_URL`, `ENV_TEST_GRAFANA_URL`, `ENV_TEST_LOKI_URL` with localhost defaults. Remaining: (1) remove `mc_webtransport_url` from `ClusterConnection` — MC/MH endpoints should come from GC join responses, (2) tag the invalid-token negative test as local-only or discover MC endpoint dynamically.

## Service Credential Management

- [ ] **AC admin API for service credentials**: Add `POST /api/v1/admin/service-credentials` to AC for registering/updating service credentials (client_id, scopes, service_type). Idempotent upsert. Currently service credentials are seeded via raw SQL in `infra/kind/scripts/setup.sh:seed_test_data()`, which has caused bugs (missing `internal:meeting-token` scope for GC) because credential config is decoupled from the code that requires it.
- [ ] **Per-service credential registration Jobs**: Each service (GC, MC, MH) should own its credential registration via a K8s Job in its Kustomize base. The Job calls the AC admin API to register the service's client_id and required scopes. Deploy pipeline ensures AC is ready before downstream services deploy. This way, adding a new scope (e.g., `internal:meeting-token`) is part of the same PR that adds the endpoint requiring it.
- [ ] **Remove setup.sh seed_test_data**: Once the admin API and registration Jobs exist, remove the raw SQL credential seeding from `infra/kind/scripts/setup.sh` and `infra/docker/postgres/init.sql`.

## Client Architecture

- [ ] **Evaluate HTTP/3 for AC**: AC currently serves HTTP/1.1 (TCP-based). For consistency with GC, evaluate adding HTTP/3 support. Low priority — client is protocol-agnostic via `fetch()`. Follow-up to ADR-0003.

## Dashboard Presentation Debt (ADR-0029)

- [ ] **AC overview duplicate "Tokens Issued" stat panel**: Panel id=39 (Traffic Summary row) and panel id=5 (Overview row) both show `sum(increase(ac_token_issuance_total{...}[$__range]))`. Remove panel id=5. Location: `infra/grafana/dashboards/ac-overview.json`
- [ ] **AC overview "Overview" row inconsistent with GC/MC**: AC has a 4-panel Overview stat row (Request Rate, Error Rate, p95 Latency, Tokens Issued) that doesn't exist in GC or MC overviews. Consider removing or aligning. Location: `infra/grafana/dashboards/ac-overview.json`
- [ ] **Timeseries panel titles say "Rate" for increase() panels**: Many panels across AC/GC/MC overviews still titled "...Rate" (e.g., "Request Rate by Endpoint") while using `increase()`. Units are correct (`short`), but titles are misleading. Large cosmetic rename. Locations: `infra/grafana/dashboards/{ac,gc,mc}-overview.json`

## Observability Debt

- [x] **Stale metric names in MC runbooks**: Fixed in `docs/runbooks/mc-incident-response.md` (8 refs: 6 PromQL `mc_message_processing_duration_seconds`, 1 PromQL `mc_gc_heartbeat_duration_seconds`, 1 grep pattern). `mc-deployment.md` had 0 stale refs (TODO entry was incorrect). Fixed in task 17.
- [x] **MC runbook missing join scenarios**: Added Scenario 8 (join failures), Scenario 9 (WebTransport rejections), and Scenario 10 (JWT validation failures) to `mc-incident-response.md`. Anchors match `mc-alerts.yaml` runbook_url references. Fixed in task 17.
- [ ] **Cross-service status label inconsistency**: GC uses `status="error"` for failures (HTTP convention) while MC uses `status="failure"` (binary convention). Both are internally consistent. Standardize if/when a cross-service alerting layer is added.

## Rate Limiting

- [x] **AC: Make rate limit constants env-configurable**: All 4 rate limit constants (login + registration) now env-configurable via `AC_RATE_LIMIT_WINDOW_MINUTES`, `AC_RATE_LIMIT_MAX_ATTEMPTS`, `AC_REGISTRATION_RATE_LIMIT_WINDOW_MINUTES`, `AC_REGISTRATION_RATE_LIMIT_MAX_ATTEMPTS`. Relaxed values set in Kind configmap.
- [ ] **GC: Wire up rate limiting middleware**: `crates/gc-service/src/config.rs` has `RATE_LIMIT_RPM` (default 100) and `GcError::RateLimitExceeded` exists, but no middleware enforces it. Add a tower rate limiting layer (e.g., governor) to routes, especially the public guest token endpoint (`/api/v1/meetings/{code}/guest-token`).
- [ ] **MC: Evaluate rate limiting needs**: MC has no rate limiting. WebTransport connections are long-lived so per-request limiting is less relevant, but the gRPC endpoint from GC should have some protection against runaway reconnection storms.

## Infrastructure Validation in Devloops

- [ ] **Deploy-step validation**: Devloops that modify K8s manifests, kustomization files, setup.sh, or other deploy infrastructure currently have no way to validate their changes actually work — issues like Kustomize path restrictions, postgres security context incompatibilities, and selector mutation only surface when running `setup.sh` against a real cluster. Need to figure out how to incorporate deploy validation into the devloop workflow (e.g., `kustomize build` dry-run, Kind cluster in CI, or a lightweight deploy-test step).

## Developer Experience

- [ ] **Resumable setup.sh**: Add a `--resume` flag to `infra/kind/scripts/setup.sh` that brings the cluster up to date without destroying it. Skip cluster creation if cluster exists, skip namespace creation if namespaces exist, skip image build+load if image tag unchanged, let `kubectl apply -k` handle idempotent infra updates. Currently any infra change requires a full teardown+rebuild (~5 min), when most steps could be skipped.
- [x] **Pre-load third-party images into Kind**: Third-party images (postgres, redis, prometheus, grafana, loki, promtail, kube-state-metrics, node-exporter) are pulled from the internet by the Kind node on every cluster creation. Pull to host Docker/Podman cache first, then `kind load` into the cluster. Makes subsequent cluster recreations faster and offline-capable. Location: `infra/kind/scripts/setup.sh:preload_third_party_images()`.
- [ ] **Skip unchanged service image builds**: `build_image` runs `docker build` for all 4 services on every `setup.sh` invocation, even when source code hasn't changed. The `COPY . .` planner stage uses the entire project root as build context, so changing any file invalidates all services' Docker caches. Two improvements: (a) add `.dockerignore` or narrow build context per service so changing GC doesn't invalidate AC's cache, (b) content-hash the relevant source files (`crates/{service}/`, `crates/common/`, `Cargo.toml`, `Cargo.lock`) and skip `build_image` + `kind load` entirely if the hash matches the previously-built image. Location: `infra/kind/scripts/setup.sh:build_image()`.

## Multi-Cluster Networking (Production)

- [ ] **Per-pod externally-routable addresses for MC/MH**: MC and MH register per-pod advertise addresses with GC (`_ADVERTISE_ADDRESS` config fields, added in `8266acc`). In Kind dev, these use pod IPs via downward API, which works single-cluster. In production, GC/MC/MH may be in different clusters and clients connect directly to MC/MH — pod IPs won't be routable. Needs a `/debate` to choose an infrastructure pattern (headless service + ExternalDNS, per-pod ingress, service mesh, etc.) and design TLS/DNS strategy. Affects: deployment model (StatefulSet vs Deployment), DNS, TLS certificates (wildcard vs per-pod), GC routing logic, client SDK connection. Depends on cloud provider selection. Locations: `crates/mc-service/src/config.rs` (grpc/webtransport_advertise_address), `crates/mh-service/src/config.rs` (same), `infra/services/mc-service/deployment.yaml`, `infra/services/mh-service/deployment.yaml`.

## Code Quality

- [ ] **dead_code lint cleanup**: Review `#[allow(dead_code)]` attributes across `crates/ac-service/src/` once more code paths are exercised by binaries
