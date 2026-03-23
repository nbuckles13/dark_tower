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
- [ ] **GC integration test fixture duplication**: `crates/gc-service/tests/participant_tests.rs`, `meeting_create_tests.rs`, `meeting_tests.rs` each re-implement org/user/meeting INSERT helpers. Within `participant_tests.rs`, `create_test_fixtures_with_status` and `create_test_fixtures_with_max` duplicate the same org/user/meeting INSERT logic differing only in parameterized fields. Consider a shared `tests/common/mod.rs` or `gc-test-utils` module.

### From ADR-0010 Phase 4a Review (2026-01-31)

- [ ] **HealthStatus::from_proto() inconsistency**: MH uses inline match with `Pending` default, MC uses centralized method with `Unhealthy` default. Location: `crates/gc-service/src/grpc/mh_service.rs`
- [ ] **gRPC input validation duplication**: MC and MH services duplicate validation logic (~100 lines). Locations: `crates/gc-service/src/grpc/mc_service.rs`, `crates/gc-service/src/grpc/mh_service.rs`
- [ ] **Heartbeat interval constants**: Defined in 3 places with different names. Locations: `mc_service.rs`, `mh_service.rs`, `meeting-controller/gc_client.rs`

## Env-Test Self-Sufficiency

- [ ] **AC org provisioning endpoint**: Add an admin/internal API to AC for creating organizations. Env-tests should create their own test org via this endpoint instead of depending on pre-seeded data in `infra/docker/postgres/init.sql`.
- [ ] **Remove init.sql seed data**: Once the AC provisioning endpoint exists, remove the `devtest` org/user seed logic from `infra/docker/postgres/init.sql` and update env-tests (20, 21, 23) to self-provision via the API.

## Client Architecture

- [ ] **Evaluate HTTP/3 for AC**: AC currently serves HTTP/1.1 (TCP-based). For consistency with GC, evaluate adding HTTP/3 support. Low priority — client is protocol-agnostic via `fetch()`. Follow-up to ADR-0003.

## Code Quality

- [ ] **dead_code lint cleanup**: Review `#[allow(dead_code)]` attributes across `crates/ac-service/src/` once more code paths are exercised by binaries
