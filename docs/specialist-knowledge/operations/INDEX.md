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
- Guard runner → `scripts/guards/run-guards.sh`
- Application metrics guard → `scripts/guards/simple/validate-application-metrics.sh`

## Code Locations — Deployment & K8s
- Kind cluster config → `infra/kind/kind-config.yaml`
- Kind setup/iterate/teardown → `infra/kind/scripts/`
- Per-service manifests (deployment, netpol, PDB) → `infra/services/{ac,gc,mc}-service/`
- Alert rules → `infra/docker/prometheus/rules/{gc,mc}-alerts.yaml`
- Grafana dashboards → `infra/grafana/dashboards/`

## Code Locations — Operational Scripts
- Dev cert generation → `scripts/generate-dev-certs.sh`
- Master key generation → `scripts/generate-master-key.sh`
- Service registration → `scripts/register-service.sh`

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
- Participant model → `crates/gc-service/src/models/mod.rs:Participant`

## Code Locations — Observability
- GC metrics recorder → `crates/gc-service/src/observability/metrics.rs`
- GC metrics catalog → `docs/observability/metrics/gc-service.md`
- MC metrics catalog → `docs/observability/metrics/mc-service.md`
- MC health probes (commented, Phase 6h) → `infra/services/mc-service/deployment.yaml:109`

## Code Locations — Token Claims (shared)
- Meeting/Guest token claims → `crates/common/src/jwt.rs:MeetingTokenClaims`, `GuestTokenClaims`
- AC token issuance (meeting/guest) → `crates/ac-service/src/handlers/internal_tokens.rs`

## Integration Seams
- Env-tests (cluster validation) → `crates/env-tests/`
- Metric catalogs (guard cross-ref) → `docs/observability/metrics/`
- NetworkPolicy cross-refs → `infra/services/*/network-policy.yaml`
