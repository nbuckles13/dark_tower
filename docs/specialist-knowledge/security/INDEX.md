# Security Navigation

## Architecture & Design
- Service authentication (OAuth 2.0 Client Credentials) → ADR-0003
- Token lifetime & refresh → ADR-0007
- Key rotation (signing keys, grace periods) → ADR-0008
- User auth & meeting access → ADR-0020
- No-panic policy → ADR-0002
- MC session binding & HKDF key hierarchy → ADR-0023 (Section 1)
- Approved algorithms → ADR-0027
- Client architecture (E2EE, key management, supply chain) → ADR-0028 (Sections 5, 1)

## Code Locations
- JWT signing/verification → `crates/ac-service/src/crypto/mod.rs:sign_jwt()`, `verify_jwt()`
- Key encryption at rest → `crates/ac-service/src/crypto/mod.rs:encrypt_private_key()`
- Bcrypt hash/verify → `crates/ac-service/src/crypto/mod.rs:hash_client_secret()`, `verify_client_secret()`
- Token issuance → `crates/ac-service/src/services/token_service.rs:issue_service_token()`, `issue_user_token()`
- Security config bounds → `crates/ac-service/src/config.rs`
- JWT size constant → `crates/common/src/jwt.rs:MAX_JWT_SIZE_BYTES`
- Shared claims types (PII-redacted Debug) → `crates/common/src/jwt.rs:ServiceClaims`, `UserClaims`, `MeetingTokenClaims`, `GuestTokenClaims::validate()`
- Meeting token enums → `crates/common/src/jwt.rs:ParticipantType`, `MeetingRole`
- Token manager (secure constructor) → `crates/common/src/token_manager.rs:new_secure()`
- Common JWKS client → `crates/common/src/jwt.rs:JwksClient`
- Common JWT validator (generic, EdDSA) → `crates/common/src/jwt.rs:JwtValidator::validate()`
- Common JWT verify token → `crates/common/src/jwt.rs:verify_token()`
- Common HasIat trait (compile-time iat enforcement) → `crates/common/src/jwt.rs:HasIat`
- Common kid extraction (8KB size check first) → `crates/common/src/jwt.rs:extract_kid()`
- Common iat validation (clock skew bounded) → `crates/common/src/jwt.rs:validate_iat()`
- GC JWT validation (thin wrapper) → `crates/gc-service/src/auth/jwt.rs:validate()`, `validate_user()`
- GC JWKS re-export → `crates/gc-service/src/auth/jwks.rs`
- GC `From<JwtError>` error mapping → `crates/gc-service/src/errors.rs`
- GC auth middleware → `crates/gc-service/src/middleware/auth.rs:require_auth()`, `require_user_auth()`
- GC CSPRNG generators → `crates/gc-service/src/handlers/meetings.rs:generate_meeting_code()`, `generate_join_token_secret()`
- GC role enforcement constants → `crates/gc-service/src/handlers/meetings.rs:MEETING_CREATE_ROLES`
- GC join status allowlist → `crates/gc-service/src/handlers/meetings.rs:join_meeting()`, `get_guest_token()`
- GC join metrics (bounded error_type labels) → `crates/gc-service/src/observability/metrics.rs:record_meeting_join()`
- MC join flow metrics (bounded labels, no PII) → `crates/mc-service/src/observability/metrics.rs:record_session_join()`, `record_jwt_validation()`, `record_webtransport_connection()`
- MC error_type_label (static str from enum, not error messages) → `crates/mc-service/src/errors.rs:error_type_label()`
- MC metrics catalog (cardinality bounds) → `docs/observability/metrics/mc-service.md`
- MC join flow dashboard panels → `infra/grafana/dashboards/mc-overview.json`
- GC atomic org limit CTE → `crates/gc-service/src/repositories/meetings.rs:create_meeting_with_limit_check()`
- GC meeting activation + audit logging → `crates/gc-service/src/repositories/meetings.rs:activate_meeting()`, `log_audit_event()`
- GC participant tracking (DB CHECK + partial unique index) → `crates/gc-service/src/repositories/participants.rs`, `migrations/20260322000001_add_participant_tracking.sql`
- MC JWT validation (thin wrapper, meeting + guest) → `crates/mc-service/src/auth/mod.rs:McJwtValidator`
- MC token_type enforcement (anti-confusion) → `crates/mc-service/src/auth/mod.rs:validate_meeting_token()` (token_type == "meeting"), `validate_guest_token()` (delegates to `GuestTokenClaims::validate()`)
- MC `From<JwtError>` error mapping → `crates/mc-service/src/errors.rs` (ServiceUnavailable→Internal, others→JwtValidation)
- MC JWKS config (scheme-validated URL) → `crates/mc-service/src/config.rs:ac_jwks_url`
- MC gRPC auth interceptor (service tokens, separate from meeting tokens) → `crates/mc-service/src/grpc/auth_interceptor.rs`
- MC session binding actors → `crates/mc-service/src/actors/session.rs`
- MC WebTransport connection handler (join flow, JWT gate, framing) → `crates/mc-service/src/webtransport/connection.rs:handle_connection()`
- MC WebTransport accept loop (capacity bound, TLS termination) → `crates/mc-service/src/webtransport/server.rs:WebTransportServer`
- MC WebTransport signaling encoding → `crates/mc-service/src/webtransport/handler.rs:encode_participant_update()`
- MC ParticipantActor (per-participant, disconnect notification) → `crates/mc-service/src/actors/participant.rs:ParticipantActor`
- MC MeetingActor join + binding token generation → `crates/mc-service/src/actors/meeting.rs:handle_join()`
- MC JoinConnection routing (controller → meeting) → `crates/mc-service/src/actors/controller.rs:JoinConnection` handler

## TLS & Certificates
- Dev cert generation (ECDSA P-256 CA) → `scripts/generate-dev-certs.sh`
- MC TLS Secret + volume mount (defaultMode 0400) → `infra/services/mc-service/tls-secret.yaml`, `deployment.yaml`
- MC WebTransport UDP ingress + Kind mapping → `infra/services/mc-service/network-policy.yaml`, `infra/kind/kind-config.yaml`

## Integration Seams
- AC JWKS → common `JwksClient` → GC `JwtValidator` + MC `McJwtValidator` (meeting/guest tokens via WebTransport)
- GC→MC gRPC service tokens → `crates/mc-service/src/grpc/auth_interceptor.rs` (separate from meeting token path)
- GC user-auth routes → `crates/gc-service/src/routes/mod.rs:build_routes()`
- Credential leak guards → `scripts/guards/simple/no-secrets-in-logs.sh`

## Runbooks & Audit
- GC security scenarios (8-9) → `docs/runbooks/gc-incident-response.md`
- Bcrypt cost / `.expose_secret()` → `crates/ac-service/src/`; fail-open env-tests → `crates/env-tests/tests/`
