# Security Navigation

## Architecture & Design
- Service authentication (OAuth 2.0 Client Credentials) → ADR-0003
- Token lifetime & refresh → ADR-0007 | Key rotation → ADR-0008
- User auth & meeting access → ADR-0020
- No-panic policy → ADR-0002 | Approved algorithms → ADR-0027
- MC session binding & HKDF key hierarchy → ADR-0023 (Section 1)
- Client architecture (E2EE, key management, supply chain) → ADR-0028 (Sections 5, 1)

## Code Locations — AC (Token Issuance & Crypto)
- JWT signing/verification → `crates/ac-service/src/crypto/mod.rs:sign_jwt()`, `verify_jwt()`
- Key encryption at rest → `crates/ac-service/src/crypto/mod.rs:encrypt_private_key()`
- Bcrypt hash/verify → `crates/ac-service/src/crypto/mod.rs:hash_client_secret()`, `verify_client_secret()`
- Token issuance → `crates/ac-service/src/services/token_service.rs:issue_service_token()`, `issue_user_token()`
- Security config bounds → `crates/ac-service/src/config.rs`

## Code Locations — Common (JWT Infrastructure)
- JWT size constant → `crates/common/src/jwt.rs:MAX_JWT_SIZE_BYTES`
- Claims types (PII-redacted Debug) → `crates/common/src/jwt.rs:ServiceClaims`, `UserClaims`, `MeetingTokenClaims`, `GuestTokenClaims::validate()`
- Meeting token enums → `crates/common/src/jwt.rs:ParticipantType`, `MeetingRole`
- JWKS client + JWT validator (EdDSA) → `crates/common/src/jwt.rs:JwksClient`, `JwtValidator::validate()`
- HasIat trait, kid extraction (8KB check), iat validation → `crates/common/src/jwt.rs`
- Token manager (secure constructor) → `crates/common/src/token_manager.rs:new_secure()`

## Code Locations — GC (Auth & Access Control)
- JWT validation (thin wrapper) → `crates/gc-service/src/auth/jwt.rs:validate()`, `validate_user()`
- Auth middleware → `crates/gc-service/src/middleware/auth.rs:require_auth()`, `require_user_auth()`
- CSPRNG generators → `crates/gc-service/src/handlers/meetings.rs:generate_meeting_code()`, `generate_join_token_secret()`
- Role enforcement + join status allowlist → `crates/gc-service/src/handlers/meetings.rs`
- Atomic org limit CTE → `crates/gc-service/src/repositories/meetings.rs:create_meeting_with_limit_check()`
- Participant tracking (DB CHECK + partial unique index) → `crates/gc-service/src/repositories/participants.rs`

## Code Locations — MC (JWT, WebTransport, Actors)
- MC JWT validation (meeting + guest) → `crates/mc-service/src/auth/mod.rs:McJwtValidator`
- Token_type anti-confusion → `validate_meeting_token()` (type == "meeting"), `validate_guest_token()` (delegates to `GuestTokenClaims::validate()`)
- JWKS config (scheme-validated URL) → `crates/mc-service/src/config.rs:ac_jwks_url`
- gRPC auth interceptor (service tokens) → `crates/mc-service/src/grpc/auth_interceptor.rs`
- WebTransport connection handler (join flow, JWT gate) → `crates/mc-service/src/webtransport/connection.rs:handle_connection()`
- WebTransport accept loop (capacity bound, TLS) → `crates/mc-service/src/webtransport/server.rs:WebTransportServer`
- Session binding actors → `crates/mc-service/src/actors/session.rs`
- MeetingActor join + binding token → `crates/mc-service/src/actors/meeting.rs:handle_join()`

## Code Locations — Observability (Security-Relevant)
- MC join metrics (bounded labels, no PII) → `crates/mc-service/src/observability/metrics.rs`
- MC error_type_label (static str from enum) → `crates/mc-service/src/errors.rs:error_type_label()`
- GC/MC alert rules (no PII) → `infra/docker/prometheus/rules/{gc,mc}-alerts.yaml`
- GC/MC dashboards (PII-free queries) → `infra/grafana/dashboards/{gc,mc}-overview.json`

## TLS & Certificates
- Dev cert generation (ECDSA P-256 CA) → `scripts/generate-dev-certs.sh`
- MC TLS Secret + volume mount (defaultMode 0400) → `infra/services/mc-service/tls-secret.yaml`, `deployment.yaml`
- MC WebTransport UDP ingress + Kind mapping → `infra/services/mc-service/network-policy.yaml`, `infra/kind/kind-config.yaml`

## Health & Probes
- MC health state (liveness/readiness) → `crates/mc-service/src/observability/health.rs:health_router()`
- MC K8s probes (`/health`, `/ready` on port 8081) → `infra/services/mc-service/deployment.yaml`
- MC health port NetworkPolicy (Prometheus-only ingress) → `infra/services/mc-service/network-policy.yaml`

## Integration Seams
- AC JWKS → common `JwksClient` → GC `JwtValidator` + MC `McJwtValidator` (meeting/guest tokens via WebTransport)
- GC→MC gRPC service tokens → `crates/mc-service/src/grpc/auth_interceptor.rs` (separate from meeting token path)
- Credential leak guards → `scripts/guards/simple/no-secrets-in-logs.sh`

## Test Coverage (Security-Relevant)
- MC join integration tests (JWT through full WebTransport path) → `crates/mc-service/tests/join_tests.rs`
  - Expired, garbage, wrong key, wrong meeting_id → all Unauthorized; error opacity asserted (no "mismatch" leak)
- MC JWT unit tests (token confusion, role tampering) → `crates/mc-service/src/auth/mod.rs` (#[cfg(test)])
- Shared JWT test fixtures (Ed25519 keypair, JWKS mock) → `crates/mc-test-utils/src/jwt_test.rs`
  - TestKeypair: deterministic seeds, `dangerous-configuration` scoped to `[dev-dependencies]` only
- GC join integration tests (service token rejection) → `crates/gc-service/tests/meeting_tests.rs`

## Production Bug Fixes (Security-Adjacent)
- send_error() stream flush → `crates/mc-service/src/webtransport/connection.rs:543` (stream.finish())
  - Without flush, Unauthorized/InvalidRequest error responses lost on QUIC stream drop (task 15)

## Runbooks & Audit
- GC security scenarios (8-9) → `docs/runbooks/gc-incident-response.md`
- Bcrypt cost / `.expose_secret()` → `crates/ac-service/src/`; fail-open env-tests → `crates/env-tests/tests/`
