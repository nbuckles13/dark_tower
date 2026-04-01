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
- Security config (bcrypt, JWT clock skew, rate limits) → `crates/ac-service/src/config.rs`
- Rate limit config + validation → `crates/ac-service/src/config.rs:parse_rate_limit_i64()`
- Rate limiting (login + registration) → `token_service.rs`, `user_service.rs`, `auth_handler.rs`
- Rate limit K8s config → `infra/services/ac-service/configmap.yaml`, `statefulset.yaml`

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
- GC/MC alert rules + dashboards (no PII) → `infra/docker/prometheus/rules/`, `infra/grafana/dashboards/`
- Dashboard metric presentation + security stat panels → ADR-0029

## TLS & Certificates
- Dev cert generation (ECDSA P-256 CA) → `scripts/generate-dev-certs.sh`
- MC TLS volume mount (defaultMode 0400) → `infra/services/mc-service/deployment.yaml`
- MC WebTransport UDP ingress + Kind mapping → `infra/services/mc-service/network-policy.yaml`, `infra/kind/kind-config.yaml`

## Infrastructure Secrets & Network Isolation
- Imperative secret creation → `infra/kind/scripts/setup.sh:create_ac_secrets()`, `create_mc_tls_secret()`
- Kind overlay (no secrets) → `infra/kubernetes/overlays/kind/`
- Grafana/Postgres/Redis secrets + NetworkPolicy → `infra/kubernetes/observability/grafana/secret.yaml`, `infra/services/{postgres,redis}/`

## Health & Probes
- MC health state → `crates/mc-service/src/observability/health.rs:health_router()`
- MC K8s probes + health port NetworkPolicy → `infra/services/mc-service/deployment.yaml`, `network-policy.yaml`

## Integration Seams
- AC JWKS → common `JwksClient` → GC/MC `JwtValidator` (meeting/guest tokens via WebTransport)
- GC→MC gRPC service tokens → `crates/mc-service/src/grpc/auth_interceptor.rs`
- Credential leak guards → `scripts/guards/simple/no-secrets-in-logs.sh`
- Kustomize security guards (R-18 securityContext, R-19 empty secrets) → `scripts/guards/simple/validate-kustomize.sh`

## Test Coverage (Security-Relevant)
- MC join tests (JWT, error opacity) + JWT unit tests → `crates/mc-service/tests/join_tests.rs`, `src/auth/mod.rs`
- JWT test fixtures (Ed25519, JWKS mock) → `crates/mc-test-utils/src/jwt_test.rs`
- GC join tests (service token rejection) → `crates/gc-service/tests/meeting_tests.rs`
