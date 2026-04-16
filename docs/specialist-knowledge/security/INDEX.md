# Security Navigation

## Architecture & Design
- Service auth (OAuth 2.0 Client Credentials, two-layer gRPC: JWKS+scope, service_type allowlist) → ADR-0003
- Token lifetime & refresh → ADR-0007 | Key rotation → ADR-0008
- User auth & meeting access → ADR-0020
- No-panic policy → ADR-0002 | Approved algorithms → ADR-0027
- MC session binding & HKDF key hierarchy → ADR-0023 (Section 1)
- Client architecture (E2EE, key management, supply chain) → ADR-0028 (Sections 5, 1)

## Code Locations — AC (Token Issuance, Crypto & Scope Alignment)
- JWT signing/verification, key encryption, bcrypt → `crates/ac-service/src/crypto/mod.rs`
- Token issuance (service_type always set) → `crates/ac-service/src/services/token_service.rs:issue_service_token()`, `issue_user_token()`
- ADR-0003 scope alignment: `default_scopes()` → `crates/ac-service/src/models/mod.rs:ServiceType`; seed SQL → `infra/kind/scripts/setup.sh:seed_test_data()`
- Scope contract tests (regression guard) → `crates/ac-service/src/models/mod.rs:test_scope_contract_*`
- Security config + rate limits → `crates/ac-service/src/config.rs` | K8s: `infra/services/ac-service/`

## Code Locations — Common (JWT Infrastructure & Shared Token Types)
- JWT claims (PII-redacted Debug), JWKS client, validator (EdDSA, size limit, kid, iat) → `crates/common/src/jwt.rs`
- Token manager (secure constructor) → `crates/common/src/token_manager.rs:new_secure()`
- Internal token types (GC→AC, `home_org_id` required) → `crates/common/src/meeting_token.rs`

## Code Locations — GC (Auth & Access Control)
- JWT validation → `crates/gc-service/src/auth/jwt.rs` | Auth middleware → `src/middleware/auth.rs`
- CSPRNG + role enforcement → `crates/gc-service/src/handlers/meetings.rs`
- Atomic org limit CTE → `crates/gc-service/src/repositories/meetings.rs:create_meeting_with_limit_check()`
- Participant tracking (DB CHECK + partial unique) → `crates/gc-service/src/repositories/participants.rs`

## Code Locations — MC (JWT, WebTransport, Actors, MH Client)
- MC JWT validation + token_type anti-confusion → `crates/mc-service/src/auth/mod.rs:McJwtValidator`
- gRPC two-layer auth `McAuthLayer` → `crates/mc-service/src/grpc/auth_interceptor.rs`: Layer 1 JWKS+scope (`service.write.mc`), Layer 2 service_type allowlist (MeetingControllerService→GC, MediaCoordinationService→MH), claims injected into extensions. Dead `McAuthInterceptor` removed.
- MC→MH OAuth Bearer auth (TokenReceiver, add_auth) → `crates/mc-service/src/grpc/mh_client.rs:MhClient`
- MediaCoordinationService (MH→MC, input validation) → `crates/mc-service/src/grpc/media_coordination.rs`
- MH connection registry (bound: 1000/meeting) + UTF-8 safe truncation (`floor_char_boundary`) → `mh_connection_registry.rs`, `webtransport/connection.rs:handle_client_message()`
- WebTransport (connection handler, accept loop, TLS, join flow, JWT gate, capacity) → `crates/mc-service/src/webtransport/`
- Join fail-closed on missing MH data (generic client error) → `connection.rs:build_join_response()`, `errors.rs:MhAssignmentMissing`
- MH assignment store (Redis, no credentials stored) → `crates/mc-service/src/redis/client.rs:MhAssignmentStore`, `MhAssignmentData`
- Session binding + join → `crates/mc-service/src/actors/session.rs`, `meeting.rs:handle_join()`

## Code Locations — MH (Auth, OAuth, TLS, Outbound Clients)
- gRPC two-layer auth `MhAuthLayer` → `crates/mh-service/src/grpc/auth_interceptor.rs`: Layer 1 JWKS+scope (`service.write.mh`), Layer 2 service_type allowlist (MediaHandlerService→MC), `classify_jwt_error()` for bounded failure_reason, claims injected into extensions. Dead `MhAuthInterceptor` removed. OAuth config → `config.rs`
- TLS validation + Bearer auth + error sanitization → `config.rs`, `gc_client.rs`, `errors.rs`
- MH→MC OAuth Bearer auth (TokenReceiver, add_auth, retry with auth short-circuit) → `crates/mh-service/src/grpc/mc_client.rs:McClient`
- MC notification wiring (fire-and-forget connect/disconnect) → `crates/mh-service/src/webtransport/connection.rs:spawn_notify_connected()`

## Code Locations — Observability (Security-Relevant)
- MC/MH metrics (bounded labels, no PII) → `crates/mc-service/src/observability/metrics.rs` (+ mh) | ADR-0029
- Layer 2 rejection metrics: `mc_caller_type_rejected_total` → `mc metrics.rs`; `mh_caller_type_rejected_total` → `mh metrics.rs`; both via `record_caller_type_rejected(grpc_service, expected_type, actual_type)`
- MH `mh_jwt_validations_total{result, token_type, failure_reason}` (bounded via `classify_jwt_error()`) → `mh metrics.rs`; catalog → `docs/observability/metrics/mh-service.md`; dashboard → `mh-overview.json`

## TLS & Certificates
- Dev cert generation (ECDSA P-256 CA, MC + MH certs) → `scripts/generate-dev-certs.sh`
- MC/MH TLS volume mounts (defaultMode 0400) → `infra/services/{mc,mh}-service/{mc,mh}-{0,1}-deployment.yaml`
- WebTransport UDP ingress + Kind mapping → `infra/services/{mc,mh}-service/network-policy.yaml`, `infra/kind/kind-config.yaml`

## Advertise Addresses (MC + MH → GC Registration)
- gRPC: K8s downward API `status.podIP` | WebTransport: per-instance env var from ConfigMap | Per-instance NodePort Services expose only UDP WebTransport | Registration → `gc_client.rs:register()`

## Devloop Container & Cluster Helper Security
- Container isolation → ADR-0025; Cluster helper (trust, socket auth, networking) → ADR-0030
- Env-test URL validation → `crates/env-tests/src/cluster.rs:parse_host_port()`, `ClusterPorts::from_env()`
- Helper binary (arg safety) → `crates/devloop-helper/src/commands.rs`; Auth token → `src/auth.rs`
- Status (read-only, auth-gated) → `commands.rs:cmd_status()`; Gateway IP → `validate_gateway_ip()` | Socket auth + API allowlist → ADR-0030
- Kind NodePort listen address → `infra/kind/kind-config.yaml.tmpl`; Wrapper → `infra/devloop/devloop.sh`

## Infrastructure Secrets & Network Isolation
- Imperative secret creation → `setup.sh:create_ac_secrets()`, `create_mc_tls_secret()`, `create_mh_secrets()`, `create_mh_tls_secret()`
- Input validation (cluster name, DT_PORT_MAP, DT_HOST_GATEWAY_IP) → `infra/kind/scripts/setup.sh` (top), `teardown.sh` (top)
- ConfigMap advertise-address patching (devloop mode) → `infra/kind/scripts/setup.sh:deploy_mc_service()`, `deploy_mh_service()`
- Single-service rebuild with allowlist → `infra/kind/scripts/setup.sh:deploy_only_service()`
- Network policies (per-service ingress/egress) → `infra/services/{ac,gc,mc,mh}-service/network-policy.yaml`; MC↔MH gRPC: MC→MH:50053, MH→MC:50052
- Kind overlay (no secrets) + supporting infra → `infra/kubernetes/overlays/kind/`, `infra/services/{postgres,redis}/`

## Health, Probes & Seams
- MC/MH health + K8s probes → `src/observability/health.rs` | Auth chain: AC JWKS → GC/MC/MH | Guards → `scripts/guards/simple/`
