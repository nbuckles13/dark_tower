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
- Security config + rate limits → `crates/ac-service/src/config.rs` | K8s: `infra/services/ac-service/`
- Rate limiting (login + registration) → `token_service.rs`, `user_service.rs`, `auth_handler.rs`

## Code Locations — Common (JWT Infrastructure & Shared Token Types)
- JWT claims (PII-redacted Debug) → `crates/common/src/jwt.rs:ServiceClaims`, `UserClaims`, `MeetingTokenClaims`, `GuestTokenClaims`
- JWKS client + JWT validator (EdDSA) → `crates/common/src/jwt.rs:JwksClient`, `JwtValidator::validate()`
- Size limit, kid extraction, iat validation → `crates/common/src/jwt.rs:MAX_JWT_SIZE_BYTES`
- Token manager (secure constructor) → `crates/common/src/token_manager.rs:new_secure()`
- Internal token types (GC→AC contract, `home_org_id` always required) → `crates/common/src/meeting_token.rs` — re-exported by GC `ac_client.rs` + AC `models/mod.rs`

## Code Locations — GC (Auth & Access Control)
- JWT validation (thin wrapper) → `crates/gc-service/src/auth/jwt.rs:validate()`, `validate_user()`
- Auth middleware → `crates/gc-service/src/middleware/auth.rs:require_auth()`, `require_user_auth()`
- CSPRNG generators → `crates/gc-service/src/handlers/meetings.rs:generate_meeting_code()`, `generate_join_token_secret()`
- Role enforcement + join status allowlist → `crates/gc-service/src/handlers/meetings.rs`
- Atomic org limit CTE → `crates/gc-service/src/repositories/meetings.rs:create_meeting_with_limit_check()`
- Participant tracking (DB CHECK + partial unique index) → `crates/gc-service/src/repositories/participants.rs`

## Code Locations — MC (JWT, WebTransport, Actors)
- MC JWT validation (meeting + guest) → `crates/mc-service/src/auth/mod.rs:McJwtValidator`
- Token_type anti-confusion → `validate_meeting_token()`, `validate_guest_token()`
- gRPC auth interceptor (service tokens) → `crates/mc-service/src/grpc/auth_interceptor.rs`
- WebTransport connection handler (join flow, JWT gate) → `crates/mc-service/src/webtransport/connection.rs:handle_connection()`
- WebTransport accept loop (capacity bound, TLS) → `crates/mc-service/src/webtransport/server.rs`
- Session binding + join → `crates/mc-service/src/actors/session.rs`, `meeting.rs:handle_join()`

## Code Locations — MH (Auth, OAuth, TLS)
- gRPC auth interceptor (MC→MH) → `crates/mh-service/src/grpc/auth_interceptor.rs:MhAuthInterceptor`
- OAuth config (SecretString, Debug redaction) → `crates/mh-service/src/config.rs:Config`
- TLS cert/key validation (fail-fast) + Bearer auth (MH→GC) → `config.rs`, `gc_client.rs:add_auth()`
- TokenManager startup (30s timeout) → `crates/mh-service/src/main.rs`
- Error sanitization → `crates/mh-service/src/errors.rs:MhError::client_message()`

## Code Locations — Observability (Security-Relevant)
- MC/MH metrics (bounded labels, no PII) → `crates/mc-service/src/observability/metrics.rs` (+ mh) | ADR-0029

## TLS & Certificates
- Dev cert generation (ECDSA P-256 CA, MC + MH certs) → `scripts/generate-dev-certs.sh`
- MC/MH TLS volume mounts (defaultMode 0400) → `infra/services/{mc,mh}-service/deployment.yaml`
- WebTransport UDP ingress + Kind mapping → `infra/services/{mc,mh}-service/network-policy.yaml`, `infra/kind/kind-config.yaml`

## Advertise Addresses (MC + MH → GC Registration)
- Config-based advertise addresses (non-secret) → `{mc,mh}-service/src/config.rs` + K8s downward API `status.podIP` in deployment.yaml
- Used in `gc_client.rs:register()` + `attempt_reregistration()` — replaces old hardcoded `format!()`/`.replace()` pattern

## Infrastructure Secrets & Network Isolation
- Imperative secret creation → `setup.sh:create_ac_secrets()`, `create_mc_tls_secret()`, `create_mh_secrets()`, `create_mh_tls_secret()`
- Network policies (per-service ingress/egress) → `infra/services/{ac,gc,mc,mh}-service/network-policy.yaml`
- Kind overlay (no secrets) + supporting infra → `infra/kubernetes/overlays/kind/`, `infra/services/{postgres,redis}/`

## Health, Probes & Integration Seams
- MC/MH health + K8s probes → `crates/mc-service/src/observability/health.rs` (+ mh), `infra/services/mc-service/deployment.yaml` (+ mh)
- AC JWKS → common `JwksClient` → GC/MC `JwtValidator` (meeting/guest tokens via WebTransport)
- gRPC service token chain: GC→MC (`mc/.../auth_interceptor.rs`) → MC→MH (`mh/.../auth_interceptor.rs`)
- Credential leak guards → `scripts/guards/simple/no-secrets-in-logs.sh`
- Kustomize security guards (R-18, R-19) → `scripts/guards/simple/validate-kustomize.sh`

## Test Coverage (Security-Relevant)
- MC join tests (JWT, error opacity) → `crates/mc-service/tests/join_tests.rs` | JWT fixtures → `crates/mc-test-utils/src/jwt_test.rs`
- GC join tests (service token rejection, home_org_id regression) → `crates/gc-service/tests/meeting_tests.rs`
