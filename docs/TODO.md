# Technical Debt

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

### From ADR-0010 Phase 4a Review (2026-01-31)

- [ ] **HealthStatus::from_proto() inconsistency**: MH uses inline match with `Pending` default, MC uses centralized method with `Unhealthy` default. Location: `crates/gc-service/src/grpc/mh_service.rs`
- [ ] **gRPC input validation duplication**: MC and MH services duplicate validation logic (~100 lines). Locations: `crates/gc-service/src/grpc/mc_service.rs`, `crates/gc-service/src/grpc/mh_service.rs`
- [ ] **Heartbeat interval constants**: Defined in 3 places with different names. Locations: `mc_service.rs`, `mh_service.rs`, `meeting-controller/gc_client.rs`

## Env-Test Self-Sufficiency

- [ ] **AC org provisioning endpoint**: Add an admin/internal API to AC for creating organizations. Env-tests should create their own test org via this endpoint instead of depending on pre-seeded data in `infra/docker/postgres/init.sql`.
- [ ] **Remove init.sql seed data**: Once the AC provisioning endpoint exists, remove the `devtest` org/user seed logic from `infra/docker/postgres/init.sql` and update env-tests (20, 21, 23) to self-provision via the API.

## Client Architecture

- [ ] **Evaluate HTTP/3 for AC**: AC currently serves HTTP/1.1 (TCP-based). For consistency with GC, evaluate adding HTTP/3 support. Low priority — client is protocol-agnostic via `fetch()`. Follow-up to ADR-0003.

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

## Code Quality

- [ ] **dead_code lint cleanup**: Review `#[allow(dead_code)]` attributes across `crates/ac-service/src/` once more code paths are exercised by binaries
