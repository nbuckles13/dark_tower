# Operations Navigation

## Architecture & Design
- Infrastructure architecture (Kind, Skaffold, zero-trust) → ADR-0012
- Local development environment → ADR-0013
- Environment integration tests → ADR-0014
- Guard pipeline methodology → ADR-0015
- Validation pipeline (CI gates) → ADR-0024 (Section: Validation Pipeline)
- Containerized devloop execution → ADR-0025
- Client architecture (CI workflow, deployment, canary) → ADR-0028

## Code Locations — CI & Guards
- CI pipeline → `.github/workflows/ci.yml`
- Guard runner + application metrics guard → `scripts/guards/run-guards.sh`, `scripts/guards/simple/validate-application-metrics.sh`

## Code Locations — Deployment & K8s
- Kind cluster config (TCP/UDP port mappings) → `infra/kind/kind-config.yaml`
- Kind setup/iterate/teardown → `infra/kind/scripts/`
- Per-service manifests (deployment, netpol, PDB) → `infra/services/{ac,gc,mc}-service/`
- Alert rules → `infra/docker/prometheus/rules/{gc,mc}-alerts.yaml`
- Dev certs (CA + service TLS), master key, service registration → `scripts/generate-dev-certs.sh`, `generate-master-key.sh`, `register-service.sh`
- MC TLS secret + volume mount → `infra/services/mc-service/tls-secret.yaml`, `deployment.yaml` (volume `mc-tls` at `/etc/mc-tls`)

## Runbooks
- GC incident response (Scenarios 1-9) → `docs/runbooks/gc-incident-response.md`
- GC deployment, rollback, smoke tests → `docs/runbooks/gc-deployment.md`
- MC incident response (Scenarios 1-7) → `docs/runbooks/mc-incident-response.md`
- MC deployment, rollback, smoke tests → `docs/runbooks/mc-deployment.md`
- AC incident response → `docs/runbooks/ac-service-incident-response.md`
- AC deployment → `docs/runbooks/ac-service-deployment.md`

## Code Locations — Database & Migrations
- Participant tracking migration → `migrations/20260322000001_add_participant_tracking.sql`
- ParticipantsRepository → `crates/gc-service/src/repositories/participants.rs`
- Meeting activation (scheduled→active) → `crates/gc-service/src/repositories/meetings.rs:activate_meeting()`
- Audit event logging + updated_at trigger → `crates/gc-service/src/repositories/meetings.rs:log_audit_event()`

## Code Locations — Auth & JWT (common crate)
- JWKS client, JWT validator, verify_token → `crates/common/src/jwt.rs:JwksClient`, `JwtValidator`, `verify_token()`
- JWK/JwksResponse types → `crates/common/src/jwt.rs:Jwk`, `JwksResponse`
- JwtError (unified error type) → `crates/common/src/jwt.rs:JwtError`
- Meeting/Guest token claims + validation → `crates/common/src/jwt.rs:MeetingTokenClaims`, `GuestTokenClaims::validate()`
- GC thin wrapper (JwtError→GcError mapping) → `crates/gc-service/src/auth/jwt.rs:JwtValidator`
- GC JwtError→GcError conversion → `crates/gc-service/src/errors.rs:impl From<JwtError> for GcError`
- Service auth design → ADR-0003

## Code Locations — Observability
- GC metrics recorder → `crates/gc-service/src/observability/metrics.rs`
- GC metrics catalog → `docs/observability/metrics/gc-service.md`
- GC meeting join metrics → `crates/gc-service/src/observability/metrics.rs:record_meeting_join()`
- GC overview dashboard → `infra/grafana/dashboards/gc-overview.json`
- MC health probes (Phase 6h) → `infra/services/mc-service/deployment.yaml:109`

## Code Locations — GC Routes & Handlers
- GC route definitions (public, user-auth, service-auth) → `crates/gc-service/src/routes/mod.rs`
- Meeting handlers (join, guest-token, settings, create) → `crates/gc-service/src/handlers/meetings.rs`

## Integration Seams
- Env-tests → `crates/env-tests/`
