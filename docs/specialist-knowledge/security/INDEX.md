# Security Navigation

## Architecture & Design
- Service authentication (OAuth 2.0 Client Credentials) → ADR-0003
- Token lifetime & refresh → ADR-0007 | Key rotation → ADR-0008
- User auth & meeting access → ADR-0020
- No-panic policy → ADR-0002 | Approved algorithms → ADR-0027
- MC session binding & HKDF key hierarchy → ADR-0023 (Section 1)
- Client architecture (E2EE, key management, supply chain) → ADR-0028 (Sections 5, 1)

## Code Locations — AC (Token Issuance & Crypto)
- JWT signing/verification, key encryption, bcrypt → `crates/ac-service/src/crypto/mod.rs`
- Token issuance → `crates/ac-service/src/services/token_service.rs:issue_service_token()`, `issue_user_token()`
- Security config + rate limits → `crates/ac-service/src/config.rs` | K8s: `infra/services/ac-service/`

## Code Locations — Common (JWT Infrastructure & Shared Token Types)
- JWT claims (PII-redacted Debug), JWKS client, validator (EdDSA, size limit, kid, iat) → `crates/common/src/jwt.rs`
- Size limit, kid extraction, iat validation → `jwt.rs:MAX_JWT_SIZE_BYTES` | Token manager → `token_manager.rs:new_secure()`
- Internal token types (GC→AC, `home_org_id` required) → `crates/common/src/meeting_token.rs`

## Code Locations — GC (Auth & Access Control)
- JWT validation → `crates/gc-service/src/auth/jwt.rs` | Auth middleware → `src/middleware/auth.rs`
- CSPRNG + role enforcement → `crates/gc-service/src/handlers/meetings.rs`
- Atomic org limit CTE → `crates/gc-service/src/repositories/meetings.rs:create_meeting_with_limit_check()`
- Participant tracking (DB CHECK + partial unique) → `crates/gc-service/src/repositories/participants.rs`

## Code Locations — MC (JWT, WebTransport, Actors)
- MC JWT validation + token_type anti-confusion → `crates/mc-service/src/auth/mod.rs:McJwtValidator`
- gRPC auth interceptor → `crates/mc-service/src/grpc/auth_interceptor.rs`
- WebTransport (connection handler, accept loop, TLS, join flow, JWT gate, capacity) → `crates/mc-service/src/webtransport/`
- Session binding + join → `crates/mc-service/src/actors/session.rs`, `meeting.rs:handle_join()`

## Code Locations — MH (Auth, OAuth, TLS)
- gRPC auth interceptor (MC→MH, structural-only — JWKS upgrade planned) → `crates/mh-service/src/grpc/auth_interceptor.rs:MhAuthInterceptor`
- RegisterMeeting field validation (meeting_id, mc_id, mc_grpc_endpoint) → `crates/mh-service/src/grpc/mh_service.rs:register_meeting()`
- OAuth config (SecretString, Debug redaction) → `crates/mh-service/src/config.rs:Config`
- TLS validation + Bearer auth + error sanitization → `config.rs`, `gc_client.rs`, `errors.rs`

## Code Locations — Observability (Security-Relevant)
- MC/MH metrics (bounded labels, no PII) → `crates/mc-service/src/observability/metrics.rs` (+ mh) | ADR-0029

## TLS & Certificates
- Dev cert generation (ECDSA P-256 CA, MC + MH certs) → `scripts/generate-dev-certs.sh`
- MC/MH TLS volume mounts (defaultMode 0400) → `infra/services/{mc,mh}-service/{mc,mh}-{0,1}-deployment.yaml`
- WebTransport UDP ingress + Kind mapping → `infra/services/{mc,mh}-service/network-policy.yaml`, `infra/kind/kind-config.yaml`

## Advertise Addresses (MC + MH → GC Registration)
- gRPC: K8s downward API `status.podIP` | WebTransport: per-instance env var from ConfigMap
- Per-instance NodePort Services (`{mc,mh}-service-{0,1}`) expose only UDP WebTransport
- Registration → `gc_client.rs:register()`, `attempt_reregistration()`

## Devloop Container & Cluster Helper Security
- Container isolation → ADR-0025; Cluster helper (trust, socket auth, injection safety, networking, prohibitions) → ADR-0030
- Env-test URL validation (scheme, credential rejection) → `crates/env-tests/src/cluster.rs:parse_host_port()`, `ClusterPorts::from_env()`
- Helper binary (Rust, Command::new() arg safety) → `crates/devloop-helper/src/commands.rs`
- Status command (read-only, auth-gated) → `commands.rs:cmd_status()`, `parse_pod_health()`
- Auth token (CSPRNG, constant-time compare, 0600) → `crates/devloop-helper/src/auth.rs`
- Gateway IP validation → `commands.rs:validate_gateway_ip()` | Dev-cluster client → `infra/devloop/dev-cluster`
- Socket auth, API allowlist, explicit prohibitions → ADR-0030 (Helper Process, Helper API, Explicit Prohibitions)
- Kind NodePort listen address (`${HOST_GATEWAY_IP}`) → `infra/kind/kind-config.yaml.tmpl`; Wrapper → `infra/devloop/devloop.sh`

## Infrastructure Secrets & Network Isolation
- Imperative secret creation → `setup.sh:create_ac_secrets()`, `create_mc_tls_secret()`, `create_mh_secrets()`, `create_mh_tls_secret()`
- Input validation (cluster name, DT_PORT_MAP, DT_HOST_GATEWAY_IP) → `infra/kind/scripts/setup.sh` (top), `teardown.sh` (top)
- ConfigMap advertise-address patching (devloop mode) → `infra/kind/scripts/setup.sh:deploy_mc_service()`, `deploy_mh_service()`
- Single-service rebuild with allowlist → `infra/kind/scripts/setup.sh:deploy_only_service()`
- Network policies (per-service ingress/egress) → `infra/services/{ac,gc,mc,mh}-service/network-policy.yaml`
- Kind overlay (no secrets) + supporting infra → `infra/kubernetes/overlays/kind/`, `infra/services/{postgres,redis}/`

## Health, Probes & Integration Seams
- MC/MH health + K8s probes → `src/observability/health.rs`, `infra/services/{mc,mh}-service/*-deployment.yaml`
- Auth chain: AC JWKS → common JwtValidator → GC/MC; gRPC: GC→MC→MH + MH→MC auth interceptors
- MC↔MH coordination protos (RegisterMeeting, MediaCoordinationService) → `proto/internal.proto`
- Guards → `scripts/guards/simple/no-secrets-in-logs.sh`, `validate-kustomize.sh`
- MC join + GC join tests → `crates/mc-service/tests/join_tests.rs`, `crates/gc-service/tests/meeting_tests.rs`
