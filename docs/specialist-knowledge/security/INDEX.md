# Security Navigation

## Architecture & Design
- Service auth (OAuth 2.0 Client Credentials) → ADR-0003 | Token lifetime & refresh → ADR-0007 | Key rotation → ADR-0008
- User auth & meeting access → ADR-0020 | PII / generic-error discipline → ADR-0011 | No-panic policy → ADR-0002 | Approved algorithms → ADR-0027
- MC session binding & HKDF key hierarchy → ADR-0023 (Section 1)
- Client architecture (E2EE, key management, supply chain) → ADR-0028 (Sections 5, 1)
- Service-owned dashboards and alerts → ADR-0031 | Alert-rules guard (URL exfil + annotation hygiene) → `scripts/guards/simple/validate-alert-rules.sh`
- Cross-boundary ownership (GSA, `Approved-Cross-Boundary:` trailer, intersection rule) → ADR-0024 §6 | GSA mirror → `.claude/skills/devloop/SKILL.md` §Cross-Boundary Edits, `.claude/skills/devloop/review-protocol.md` Step 0 | GSA manifest + classification guard → `scripts/guards/simple/cross-boundary-ownership.yaml`, `validate-cross-boundary-classification.sh`
- Guard pipeline as single Rust binary `dt-guard` (ReDoS-safe `regex` crate, typed `serde_yaml` schema, SHA256/digest-pinned vendor binaries) → ADR-0034 | canonical path-containment gate → `crates/dt-guard/src/common/path_safety.rs` | symlink-escape tests → `crates/dt-guard/tests/doc_cite_resolve.rs`
- Client credential-lifetime enforcement (retention + transmission-while-token-held) → `crates/dt-guard/src/ts_retained_credentials.rs`, wrapper `scripts/guards/simple/ts/no-retained-credentials.sh` | semantic lens → `scripts/guards/semantic/checks.md` §Client Credential Lifetime | token-based join (no password retained) → `packages/sdk-core/src/session/events.ts::TokenCredentials`, `packages/web-app/src/lib/types.ts::AuthSession`

## Dependency Audit & Supply Chain
- Layer-6 audit dep-change gate (fires only on true dep manifests) → `scripts/lang/_audit_gate.sh:audit_dep_changed_rust`/`audit_dep_changed_ts`; glob predicate → `scripts/lang/_changed_helpers.sh:diff_touches_glob`; ambient/diff-less safety net → `.github/workflows/audit-scheduled.yml` | ADR-0033 §3, §11
- Audit-suppression governance (fail-secure to zero, drift-checked) → `audit-suppressions.toml`, `.pnpm-audit-ignore.json`, guard `scripts/audit-suppressions-check.sh` | fix-not-suppress transitive overrides → root `package.json` `pnpm.overrides`
- Toolchain pin as security property (engine-strict, loud unsupported-engine) → `.npmrc`, root `package.json` `engines.node`, `.nvmrc`, `infra/devloop/Dockerfile`

## Code Locations — AC (Token Issuance & Crypto)
- JWT signing/verification, key encryption, bcrypt → `crates/ac-service/src/crypto/mod.rs`
- Token issuance → `crates/ac-service/src/services/token_service.rs:issue_service_token()`, `issue_user_token()`
- Meeting-token display-name (fail-closed: no users row → generic `NotFound`, never mints nameless token) → `crates/ac-service/src/handlers/internal_tokens.rs:resolve_meeting_display_name()`
- Security config + rate limits (registration SSoT `DEFAULT_REGISTRATION_RATE_LIMIT_*`) → `crates/ac-service/src/config.rs` | K8s: `infra/services/ac-service/`

## Code Locations — Common (JWT Infrastructure & Shared Token Types)
- JWT claims (PII-redacted Debug: `display_name`/`sub`/`jti` → `[REDACTED]`), JWKS client, validator (EdDSA, size limit, kid, iat) → `crates/common/src/jwt.rs`
- Token manager (secure constructor) → `crates/common/src/token_manager.rs:new_secure()` | Internal/meeting token types (`home_org_id` required, `display_name` carrier) → `crates/common/src/meeting_token.rs`

## Code Locations — GC (Auth, Access Control, CORS, Telemetry)
- JWT validation → `crates/gc-service/src/auth/jwt.rs` | Auth middleware → `src/middleware/auth.rs`
- CORS allowlist (fail-closed, never `*` — wildcard panics router build) → `crates/gc-service/src/config.rs:Config.cors_allowed_origins` (env `CORS_ALLOWED_ORIGINS`); layer → `src/routes/mod.rs:build_cors_layer`; preflight deny-rewrite → `src/middleware/cors_observer.rs:cors_preflight_observer`; incident → `docs/runbooks/gc-incident-response.md` Scenario 12
- Telemetry proxy (user-JWT gate `require_user_auth`; PII drop + size/rate caps) → `crates/gc-service/src/handlers/telemetry.rs`, `src/services/telemetry_filter.rs:filter_metrics`
- CSPRNG + role enforcement → `crates/gc-service/src/handlers/meetings.rs` | atomic org limit CTE → `crates/gc-service/src/repositories/meetings.rs:create_meeting_with_limit_check()` | participant tracking (DB CHECK + partial unique) → `crates/gc-service/src/repositories/participants.rs`

## Code Locations — MC (JWT, WebTransport, Actors, MH Client)
- MC JWT validation + token_type anti-confusion → `crates/mc-service/src/auth/mod.rs:McJwtValidator`
- gRPC auth: structural `McAuthInterceptor` | JWKS `McAuthLayer` (scope `service.write.mc`) → `crates/mc-service/src/grpc/auth_interceptor.rs`
- MC→MH OAuth Bearer auth (TokenReceiver, add_auth, MhRegistrationClient trait) → `crates/mc-service/src/grpc/mh_client.rs`
- Async RegisterMeeting trigger (first-participant, retry+backoff, CancellationToken) → `webtransport/connection.rs:register_meeting_with_handlers()`
- MediaCoordinationService (MH→MC, input validation; idempotent re-disconnect) → `crates/mc-service/src/grpc/media_coordination.rs`
- MH connection registry (bound 1000/meeting) + UTF-8 safe truncation → `mh_connection_registry.rs`, `connection.rs:handle_client_message()`
- WebTransport (connection handler, accept loop, TLS, join flow, JWT gate, capacity) → `crates/mc-service/src/webtransport/`
- Join trust boundary: display-name `truncate_utf8` cap (`MAX_PARTICIPANT_NAME_LEN`), client `participant_name` length-check, fail-closed on missing MH data → `crates/mc-service/src/webtransport/connection.rs`; empty-claim `Participant N` fallback → `crates/mc-service/src/actors/meeting.rs:handle_join()`
- Disconnect: transport-authenticated close (not client-forgeable), raw close-reason NOT logged (`&'static str` `error_variant`) → `crates/mc-service/src/webtransport/connection.rs:run_bridge_loop`
- MH assignment store (Redis, no credentials stored) → `crates/mc-service/src/redis/client.rs:MhAssignmentStore` | session binding → `crates/mc-service/src/actors/session.rs`, `meeting.rs:handle_join()`; integration tests (auth JWT failure modes, WT accept-path) → `crates/mc-service/tests/`

## Code Locations — MH (Auth, OAuth, TLS)
- gRPC auth layer (JWKS, scope `service.write.mh`) → `crates/mh-service/src/grpc/auth_interceptor.rs:MhAuthLayer`
- MH JWT validator (`token_type == "meeting"` anti-confusion) → `crates/mh-service/src/auth/mod.rs:MhJwtValidator::validate_meeting_token`
- WT accept-path JWT gate → `crates/mh-service/src/webtransport/connection.rs:handle_connection()`
- OAuth config (SecretString) → `crates/mh-service/src/config.rs:Config` | TLS+Bearer → `crates/mh-service/src/grpc/gc_client.rs` | Error sanitization → `crates/mh-service/src/errors.rs` | JWKS: `infra/services/mh-service/configmap.yaml` | integration tests (auth E2E, WT accept-path, RegisterMeeting) → `crates/mh-service/tests/`

## Code Locations — Client / E2E (Credential-Leak Controls)
- Token-only join traffic assertion + failure-message `redact` helper → `packages/web-app/e2e/fixtures.ts:assertTokenOnlyJoinTraffic`
- Test-bus secret/PII whitelist projection (drops `bindingToken`/`correlationId`) → `packages/web-app/src/lib/e2eBus.ts`
- Token/meeting-code format validation → `packages/sdk-core/src/validation/limits.ts:validateUserToken`/`validateMeetingCode`
- Trace artifacts contain live tokens — gitignored, do-not-promote → `packages/web-app/e2e/README.md`, runbook `docs/runbooks/devloop-validation.md` §6.7/§8

## Observability (Security-Relevant)
- MC/MH metrics — bounded enum→`&'static str` labels, never PII/client-string/`mh_url`/raw close-reason → `crates/mc-service/src/observability/metrics.rs` (+ mh) | ADR-0029
- AC metric integration suites (audit-log failure, rate-limit, clock-skew, key-rotation, error-category) → `crates/ac-service/tests/`; failure-path adjacency API → `crates/common/src/observability/testing.rs`
- ADR-0032 GC audit patterns (authz-shift, bounded `error_type`, caller-type pinning) → `crates/gc-service/src/handlers/meetings.rs`, `docs/observability/metrics/gc-service.md` | metric-catalog debt → `docs/TODO.md` §Observability Debt

## TLS & Certificates
- Dev cert generation + fingerprint SSoT (single writer) → `scripts/generate-dev-certs.sh`; consumers → `packages/web-app/vite/fingerprints.ts`, `scripts/layer7.sh` (14-day `serverCertificateHashes` expiry cap); Playwright hash-pin (no cert-bypass flags) → `packages/web-app/playwright.config.ts`
- MC/MH TLS volume mounts (defaultMode 0400) → `infra/services/{mc,mh}-service/{mc,mh}-{0,1}-deployment.yaml`; WebTransport UDP ingress → `infra/services/{mc,mh}-service/network-policy.yaml`, `infra/kind/kind-config.yaml`; test-time self-signed PEM rigs (rcgen, SAN `localhost`/`127.0.0.1`) → `crates/mh-service/tests/common/accept_loop_rig.rs`

## Devloop Container & Cluster Helper Security
- Container isolation → ADR-0025; Cluster helper (trust, socket auth, injection safety, API allowlist, file perms) → ADR-0030
- Helper binary (arg safety, status read-only auth-gated, gateway IP validation) → `crates/devloop-helper/src/commands.rs`; Auth token (CSPRNG, constant-time compare, 0600) → `crates/devloop-helper/src/auth.rs`
- Env-test URL validation (scheme, credential rejection) → `crates/env-tests/src/cluster.rs:parse_host_port()`
- Kind NodePort listen address (`${HOST_GATEWAY_IP}`) → `infra/kind/kind-config.yaml.tmpl`; Wrapper → `infra/devloop/devloop.sh`; Dev-cluster client → `infra/devloop/dev-cluster`

## Infrastructure Secrets & Network Isolation
- Imperative secret creation → `setup.sh:create_{ac,mc_tls,mh,mh_tls}_secret()`; input validation → `infra/kind/scripts/setup.sh`, `teardown.sh`; ConfigMap advertise-address patching → `setup.sh:deploy_{mc,mh}_service()`; single-service rebuild allowlist → `setup.sh:deploy_only_service()`
- Network policies (per-service ingress/egress) → `infra/services/{ac,gc,mc,mh}-service/network-policy.yaml`; MC↔MH gRPC MC→MH:50053 / MH→MC:50052; health probes → `crates/mc-service/src/observability/health.rs`
