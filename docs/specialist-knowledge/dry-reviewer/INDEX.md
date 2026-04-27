# DRY Reviewer Navigation

## Architecture & Design
- Blocking vs tech-debt classification -> ADR-0019 (`docs/decisions/adr-0019-dry-reviewer.md`)
- Fix-or-defer review model -> ADR-0024 (`docs/decisions/adr-0024-agent-teams-workflow.md`)

## JWT Validation (Common + Thin Wrappers)
- Common JWT code (all shared logic) -> `crates/common/src/jwt.rs`
- Internal token request types (GC->AC contract) -> `crates/common/src/meeting_token.rs`
- GC thin wrapper -> `crates/gc-service/src/auth/jwt.rs` (ServiceClaims, UserClaims)
- MC thin wrapper -> `crates/mc-service/src/auth/mod.rs` (MeetingTokenClaims, GuestTokenClaims)
- MH JWKS config -> `infra/services/mh-service/configmap.yaml:AC_JWKS_URL` (shared configmap, mirrors MC pattern in `mc-service-config`)

## Per-Service Observability (Metrics & Dashboards)
- AC/GC/MC/MH metrics -> `crates/*/src/observability/metrics.rs` (per-service, not duplication)
- Alert rules -> `infra/docker/prometheus/rules/{mc,gc}-alerts.yaml`; dashboards -> ADR-0029, `infra/grafana/dashboards/`

## Integration Test Coverage
- MC tests (join_tests, actor_metrics, auth_layer, gc, media_coordination, orphan_metrics, redis_metrics, register_meeting, token_refresh, webtransport_accept_loop) -> `crates/mc-service/tests/`; GC join/guest/settings -> `crates/gc-service/tests/meeting_tests.rs`
- MC shared scaffolding (MockMhAssignmentStore, MockMhRegistrationClient, TestStackHandles, build_test_stack, seed_meeting_with_mh) -> `crates/mc-service/tests/common/mod.rs`
- MC accept-loop rig (`bind() → accept_loop()` + `write_self_signed_pems`) -> `crates/mc-service/tests/common/accept_loop_rig.rs:AcceptLoopRig` (near-clone of MH's; extraction candidate per ADR-0032 §Step 6 + TODO.md)
- MH tests (gc_integration, mc_client_integration, auth_layer_integration, register_meeting_integration, webtransport_integration) -> `crates/mh-service/tests/`
- MH shared rigs (TestKeypair, mock_mc, jwks_rig, grpc_rig, accept_loop_rig, wt_client, tokens) -> `crates/mh-service/tests/common/`
- AC tests (Step 4 metric backfill, 13 cluster files: http, bcrypt, token_validation, rate_limit, key_rotation, jwks, credential_ops, errors, token_issuance_service, token_issuance_user, internal_token, audit_log_failures, db) -> `crates/ac-service/tests/*_integration.rs`
- AC in-crate scaffolding (`make_app_state`, `seed_signing_key`, `seed_service_credential`) -> `crates/ac-service/tests/common/test_state.rs`; JWT signing helpers (`sign_service_token`, `sign_user_token`) -> `crates/ac-service/tests/common/jwt_fixtures.rs`. Uses MIN_BCRYPT_COST except `tests/bcrypt_metrics_integration.rs` (DEFAULT for histogram fidelity). TODO.md.
- GC in-crate scaffolding (`TestKeypair`, `build_pkcs8_from_seed`, `TestUserClaims`, `TestServiceClaims`) -> `crates/gc-service/tests/common/jwt_fixtures.rs` (ADR-0032 Step 5; full 3-of-3 in-place migration: `meeting_create_tests.rs`, `meeting_tests.rs`, `auth_tests.rs` all consume; attack-vector helpers `create_hs256_token`/`create_token_with_wrong_key`/`create_tampered_token` extracted as free fns operating on `&TestKeypair`).
- Shared fixtures (cross-service) -> `crates/mc-test-utils/src/jwt_test.rs`, `crates/gc-test-utils/src/server_harness.rs`, `crates/ac-test-utils/src/` (crypto_fixtures, server_harness, token_builders); MetricAssertion -> `crates/common/src/observability/testing.rs`

## Per-Service Config Parsing
- AC/GC/MC/MH config -> `crates/*/src/config.rs:Config::from_vars()` (per-service); ordinal parsing -> `crates/common/src/config.rs:parse_statefulset_ordinal()`
- Extraction candidate: `generate_instance_id(prefix)` -> 4-line pattern in GC + MC + MH config

## gRPC Auth (Cross-Service)
- MC/MH auth layers (async JWKS, R-22; legacy `*AuthInterceptor` in same files is dead in production) -> `crates/mc-service/src/grpc/auth_interceptor.rs:McAuthLayer`, `crates/mh-service/src/grpc/auth_interceptor.rs:MhAuthLayer`
- McAuthLayer/MhAuthLayer near-identical tower Layer/Service (extraction candidate in TODO.md); shared constant `common::jwt::MAX_JWT_SIZE_BYTES`

## MC gRPC Services (GC→MC + MH→MC + MC→MH)
- MC assignment (GC→MC) -> `crates/mc-service/src/grpc/mc_service.rs:McAssignmentService`; media coordination (MH→MC, R-15) -> `:grpc/media_coordination.rs:McMediaCoordinationService`
- MH connection registry (single MAX_ID_LENGTH source) -> `crates/mc-service/src/mh_connection_registry.rs:MhConnectionRegistry`
- MhRegistrationClient trait + async RegisterMeeting trigger (R-12) -> `crates/mc-service/src/grpc/mh_client.rs`, `:webtransport/connection.rs:register_meeting_with_handlers()`

## gRPC Clients (Cross-Service)
- MC GcClient (bounded retries, fast/comprehensive heartbeats) -> `crates/mc-service/src/grpc/gc_client.rs`; MC MhClient (per-call channels, no retries) -> `crates/mc-service/src/grpc/mh_client.rs`
- MH GcClient (unbounded retries, load reports) -> `crates/mh-service/src/grpc/gc_client.rs`
- Shared patterns: channel creation, `add_auth` (~10 lines, 3 call sites — extraction candidate per TODO.md), backoff constants

## Redis Abstractions (MC)
- MhAssignmentStore trait -> `crates/mc-service/src/redis/client.rs:MhAssignmentStore` (testability seam for join flow)
- FencedRedisClient -> `crates/mc-service/src/redis/client.rs:FencedRedisClient` (fenced writes, implements MhAssignmentStore)

## Health Endpoints (Cross-Service Consistency)
- MC health -> `crates/mc-service/src/observability/health.rs:health_router()` | MH -> `crates/mh-service/src/observability/health.rs` (duplicated, see TODO.md)
- GC health -> `crates/gc-service/src/routes/mod.rs:64-65`

## Per-Service Infrastructure (K8s, Docker, Kind)
- Kustomize bases -> `infra/services/{ac,gc,mc,mh}-service/kustomization.yaml`
- MC/MH per-pod Services + ConfigMaps -> `infra/services/{mc,mh}-service/` (port: `base + ordinal*2`)
- Network policies -> `infra/services/{ac,gc,mc,mh}-service/network-policy.yaml`
- Dockerfiles -> `infra/docker/{ac,gc,mc,mh}-service/Dockerfile`; Kind -> `infra/kind/`
- setup/teardown (ADR-0030) -> `infra/kind/scripts/{setup,teardown}.sh`; devloop -> `infra/devloop/devloop.sh`

## False Positive Boundaries
- Per-service error mapping (GcError vs McError vs MhError) -> required, not duplication
- MC GcClient vs MH GcClient -> different RPCs, retry strategies, heartbeat models
- AC rate limiting (DB-backed lockout) vs GC rate limiting (middleware RPM) -> different mechanisms
- common::jwt enums vs common::meeting_token -> JWT enums intentionally narrower
- AC test-side `Claims { ... }` literal repetition (5 axes of variation) -> false-positive on the literal itself; surrounding decrypt-and-sign boilerplate IS extracted to `tests/common/jwt_fixtures.rs` (`sign_service_token`, `sign_user_token`; 6 call sites). TODO.md tracks the literal-only repetition.

## Abstraction Lessons (DRY judgment calls)
- **Speculative→tightened helper iteration (Step 4 iter-2→iter-3)**: iter-1's `sign_service_token_iat_offset(..., sub, scope, iat_offset)` was speculative — pre-baked 3 of 5 axes that varied across callers, so no call site adopted it. iter-2 deleted the dead helpers but left the underlying decrypt-and-sign boilerplate inline. iter-3 re-introduced `sign_service_token(pool, master_key, &Claims)` / `sign_user_token(...)` parameterized only on the truly-fixed mechanic (key fetch + decrypt + sign), leaving claims content to callers. 6 call sites adopted. Lesson: when removing dead helpers, verify whether the duplication itself is gone or just moved inline; abstract the fixed mechanic, not speculative axes.
- **In-crate `tests/common/test_state.rs` vs `*-test-utils` (Steps 3-4)**: per-service `AppState`/`Config` builders + DB seeding belong in-crate; cross-crate fixtures (harnesses, crypto, token builders) belong in `*-test-utils`. Consolidate only when a 3rd in-crate caller appears AND shapes converge.
- **Per-crate `tests/common/` is the 3-crate sibling pattern (Steps 3-5)**: AC `tests/common/{mod.rs, jwt_fixtures.rs, test_state.rs}` (Step 4), MC `tests/common/mod.rs` (Step 3), GC `tests/common/{mod.rs, jwt_fixtures.rs}` (Step 5). All three diverge in `AppState`/repo/gRPC topology — workspace-level extraction is rejected until shapes converge. Workspace-level promotion to a `crates/test-utils-common` crate triggers when a 4th caller emerges AND the helpers stay service-private. Tracked in `docs/TODO.md` "tests/common/test_state.rs per-service test scaffolding" entry.
- **Mechanical-vs-non-mechanical migration distinction (Step 5)**: a migration is "mechanical" when it preserves observable test behavior bit-for-bit — applies even when the *call shape* changes. Worked example from Step 5: GC's attack-vector helpers (`create_hs256_token`, `create_token_with_wrong_key`, `create_tampered_token`) lived as `&self` methods on the inline `TestKeypair`. Extracting `TestKeypair` to `tests/common/jwt_fixtures.rs` and refactoring the attack helpers to free fns taking `&TestKeypair` is mechanical-spirit even though receivers change. Distinct from a non-mechanical migration like "rename `TestClaims` to `TestServiceClaims`" which has interaction with semantically-similar-but-distinct types and warrants a discrete review pass. Rule: extracting fixed mechanics is mechanical; renaming or re-shaping types is not.
- **"Delete dead helpers, abstract the fixed mechanic" (AC Step 4 iter-3 + GC Step 5)**: when removing speculative helpers, verify whether the duplication itself is gone or just moved inline. The recovery move: re-introduce a helper parameterized only on the truly-fixed mechanic (key fetch + decrypt + sign in AC; PKCS#8 encoding + Ed25519 signing in GC), leaving caller-specific axes (claims content, attack mutations) to call sites. The bound is "abstract the fixed mechanic, not speculative axes."
- **Rejected abstraction (anti-pattern reference): cross-service `record_token_refresh_metrics` consolidation (ADR-0032 Cat B, Steps 2/3/5)**: 3-line per-service dispatcher (`status = if event.success { "success" } else { "error" }; record_token_refresh(status, event.error_category, event.duration);`) with prefix-only delta (`mh_`/`mc_`/`gc_`). Cross-service consolidation via macro/generic was rejected because abstraction over a 3-line closure ADDS complexity. Resolution: per-service parallel sibling at `mh-service/src/observability/metrics.rs:126`, `mc-service/.../metrics.rs:246`, `gc-service/.../metrics.rs:302`. Each documents the cross-references. Re-evaluate only if a 4th service joins. Closing rationale captured in `docs/TODO.md` (resolved 2026-04-27 entry). Use as the canonical "abstraction below the threshold where it reduces complexity" reference when a future cross-service N-line-with-prefix-delta candidate arises.

## Tech Debt & Extractions
- Active duplication tech debt -> `docs/TODO.md` (Cross-Service Duplication section)
- Common crate + test fixtures -> `crates/common/src/`, `crates/mc-test-utils/`, `crates/gc-test-utils/`, `crates/ac-test-utils/`
