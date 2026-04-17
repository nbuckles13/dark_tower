# Security Navigation

## Architecture & Design
- Service authentication (OAuth 2.0 Client Credentials) → ADR-0003
- Token lifetime & refresh → ADR-0007 | Key rotation → ADR-0008
- User auth & meeting access → ADR-0020
- No-panic policy → ADR-0002 | Approved algorithms → ADR-0027
- MC session binding & HKDF key hierarchy → ADR-0023 (Section 1)
- Client architecture (E2EE, key management, supply chain) → ADR-0028 (Sections 5, 1)
- Service-owned dashboards and alerts → ADR-0031 | Alert-rules guard (URL exfil closure + annotation hygiene) → `scripts/guards/simple/validate-alert-rules.sh`

## Code Locations — AC (Token Issuance & Crypto)
- JWT signing/verification, key encryption, bcrypt → `crates/ac-service/src/crypto/mod.rs`
- Token issuance → `crates/ac-service/src/services/token_service.rs:issue_service_token()`, `issue_user_token()`
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
- gRPC auth: structural `McAuthInterceptor` | JWKS `McAuthLayer` (scope `service.write.mc`) → `crates/mc-service/src/grpc/auth_interceptor.rs`
- MC→MH OAuth Bearer auth (TokenReceiver, add_auth, MhRegistrationClient trait) → `crates/mc-service/src/grpc/mh_client.rs`
- Async RegisterMeeting trigger (first-participant, retry+backoff, CancellationToken) → `webtransport/connection.rs:register_meeting_with_handlers()`
- MediaCoordinationService (MH→MC, input validation; idempotent re-disconnect returns Ok to avoid retry amplification) → `crates/mc-service/src/grpc/media_coordination.rs`
- MH connection registry (bound: 1000/meeting) + UTF-8 safe truncation → `mh_connection_registry.rs`, `connection.rs:handle_client_message()`
- WebTransport (connection handler, accept loop, TLS, join flow, JWT gate, capacity) → `crates/mc-service/src/webtransport/`
- Join fail-closed on missing MH data (generic client error) → `connection.rs:build_join_response()`
- MH assignment store (Redis, no credentials stored) → `crates/mc-service/src/redis/client.rs:MhAssignmentStore`
- Session binding + join → `crates/mc-service/src/actors/session.rs`, `meeting.rs:handle_join()`; Integration tests (auth-layer JWT failure modes, WT accept-path, mocks) → `crates/mc-service/tests/`

## Code Locations — MH (Auth, OAuth, TLS)
- gRPC auth layer (JWKS, scope `service.write.mh`) → `crates/mh-service/src/grpc/auth_interceptor.rs:MhAuthLayer`
- MH JWT validator (`token_type == "meeting"` anti-confusion) → `crates/mh-service/src/auth/mod.rs:MhJwtValidator::validate_meeting_token`
- WT accept-path JWT gate → `crates/mh-service/src/webtransport/connection.rs:handle_connection()`
- OAuth config (SecretString) → `crates/mh-service/src/config.rs:Config` | TLS+Bearer → `crates/mh-service/src/grpc/gc_client.rs` | Error sanitization → `crates/mh-service/src/errors.rs` | JWKS: `infra/services/mh-service/configmap.yaml`
- Integration tests (auth E2E, WT accept-path, RegisterMeeting, rigs) → `crates/mh-service/tests/`

## Code Locations — Observability (Security-Relevant)
- MC/MH metrics (bounded labels, no PII) → `crates/mc-service/src/observability/metrics.rs` (+ mh) | ADR-0029
- AC audit-log failure real-drive pattern: `ALTER TABLE auth_events ADD CONSTRAINT ... CHECK (...) NOT VALID` (`break_auth_events_inserts`, preserves pre-INSERT SELECTs) and `DROP TABLE auth_events CASCADE` (`break_auth_events_table`, for fns with no auth_events SELECT) → `crates/ac-service/tests/audit_log_failures_integration.rs`; covers all 10 production `record_audit_log_failure` sites including `key_rotated`/`key_expired`/`scopes_updated`/`service_deactivated` (high-stakes lifecycle/privilege/revocation events)
- AC rate-limit 6-cell (gate × outcome) hard-rule pattern; snapshot-immediately-before-decision avoids cumulative-delta entanglement; registration `allowed` honestly accounts for chained auto-login emission (`assert_delta(2)`) → `crates/ac-service/tests/rate_limit_metrics_integration.rs`
- AC JWT clock-skew dual-assertion pattern (verification rejection AND metric delta AND sibling-`error_category` adjacency) for `verify_jwt` and `verify_user_jwt` → `crates/ac-service/tests/token_validation_integration.rs`
- AC key-rotation gauges from real production paths only (`initialize_signing_key`, `handle_rotate_keys`); failure-path `assert_unobserved` adjacency on the 3 signing-key gauges → `crates/ac-service/tests/key_rotation_metrics_integration.rs`
- AC per-`ErrorCategory` real-handler drives (Authentication / Authorization / Cryptographic / Internal) with real Ed25519 signing for auth-token rejection cells → `crates/ac-service/tests/errors_metric_integration.rs`
- Failure-path metric adjacency API (`assert_unobserved` symmetric across counter/gauge/histogram, `ensure_no_kind_mismatch` hardening, histogram drain-on-read caveat) → `crates/common/src/observability/testing.rs`
- AC observability orphans surfaced during ADR-0032 Step 4 — `clock_skew` cardinality drift vs catalog, `record_token_validation` Phase-4 reservation, `ac_jwks_requests_total{cache_status}` `hit`/`bypass` reservations → `docs/TODO.md` §Observability Debt
- ADR-0032 Step 5 GC audit patterns: `get_guest_token` authz-shift verification (each pre-existing predicate still gates with metric inserted BEFORE error return; `Instant::now()` precedes body-validation, no authz moved earlier; shared `gc_meeting_join_*` family discriminated by `participant=user|guest`) → `crates/gc-service/src/handlers/meetings.rs:512-639` ↔ `join_meeting:338-455`; bounded-set audit for new `error_type` (`guests_disabled`/`bad_request`) requires same-PR catalog update + cap-10 cardinality + `&'static str` literals → `docs/observability/metrics/gc-service.md:247-269`; `actual_type` single-source pinning at `crates/gc-service/src/grpc/auth_layer.rs:241` (`claims.service_type.as_deref().unwrap_or("unknown")`), test consts must match AC-issued set + `"unknown"`, never seed fresh strings → `crates/gc-service/tests/caller_type_rejected_metrics_integration.rs:34-39`; Cat B closure byte-equivalence (zero captures, body identical modulo `event`→`&event`) → `crates/gc-service/src/main.rs:124-126` ↔ `crates/gc-service/src/observability/metrics.rs:302-305`; WRAPPER-CAT-C deferral acceptable when end-to-end coverage exists elsewhere + TODO tracker with LoC estimate → `crates/gc-service/tests/jwt_validation_metrics_integration.rs:7-24`; Ed25519-from-`seed: u8` deterministic test keys, local PKCS#8 envelope, `password_hash='hashed'` placeholders → `crates/gc-service/tests/common/jwt_fixtures.rs`

## TLS & Certificates
- Dev cert generation (ECDSA P-256 CA, MC + MH certs) → `scripts/generate-dev-certs.sh`
- MC/MH TLS volume mounts (defaultMode 0400) → `infra/services/{mc,mh}-service/{mc,mh}-{0,1}-deployment.yaml`; WebTransport UDP ingress → `infra/services/{mc,mh}-service/network-policy.yaml`, `infra/kind/kind-config.yaml`
- Test-time self-signed PEM rigs (rcgen, SAN `localhost`/`127.0.0.1`, TempDir) → `crates/mh-service/tests/common/accept_loop_rig.rs`, `crates/mc-service/tests/common/accept_loop_rig.rs`

## Advertise Addresses (MC + MH → GC Registration)
- gRPC: K8s `status.podIP` | WT: per-instance env from ConfigMap | NodePort `{mc,mh}-service-{0,1}` UDP-only | Registration → `gc_client.rs:register()`

## Devloop Container & Cluster Helper Security
- Container isolation → ADR-0025; Cluster helper (trust, socket auth, injection safety, API allowlist, file perms, explicit prohibitions) → ADR-0030
- Helper binary (Command::new() arg safety, status read-only auth-gated, gateway IP validation) → `crates/devloop-helper/src/commands.rs`; Auth token (CSPRNG, constant-time compare, 0600) → `crates/devloop-helper/src/auth.rs`
- Env-test URL validation (scheme, credential rejection) → `crates/env-tests/src/cluster.rs:parse_host_port()`
- Kind NodePort listen address (`${HOST_GATEWAY_IP}`) → `infra/kind/kind-config.yaml.tmpl`; Wrapper → `infra/devloop/devloop.sh`; Dev-cluster client → `infra/devloop/dev-cluster`

## Infrastructure Secrets & Network Isolation
- Imperative secret creation → `setup.sh:create_ac_secrets()`, `create_mc_tls_secret()`, `create_mh_secrets()`, `create_mh_tls_secret()`; input validation (cluster name, DT_PORT_MAP, DT_HOST_GATEWAY_IP) → `infra/kind/scripts/setup.sh`, `teardown.sh`; ConfigMap advertise-address patching → `setup.sh:deploy_mc_service()`, `deploy_mh_service()`; single-service rebuild allowlist → `setup.sh:deploy_only_service()`
- Network policies (per-service ingress/egress) → `infra/services/{ac,gc,mc,mh}-service/network-policy.yaml`; MC↔MH gRPC: MC→MH:50053, MH→MC:50052
- MC/MH K8s health probes → `crates/mc-service/src/observability/health.rs`, `crates/mh-service/src/observability/health.rs`; Join-flow integration tests → `crates/mc-service/tests/`, `crates/gc-service/tests/`
