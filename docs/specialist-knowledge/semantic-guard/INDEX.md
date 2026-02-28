# Semantic Guard Navigation

## Architecture & Design
- Guard methodology & principles → ADR-0015 (`docs/decisions/adr-0015-principles-guards-methodology.md`)
- Agent Teams validation pipeline → ADR-0024 (`docs/decisions/adr-0024-agent-teams-workflow.md`)

## Code Locations
- Semantic check definitions → `scripts/guards/semantic/checks.md`
- Shared guard utilities → `scripts/guards/common.sh`

## Metrics Catalogs (Source of Truth for Label Validation)
- AC metrics catalog → `docs/observability/metrics/ac-service.md`
- GC metrics catalog → `docs/observability/metrics/gc-service.md`
- MC metrics catalog → `docs/observability/metrics/mc-service.md`

## Cross-Service Boundary Files
- Shared JWT types (ServiceClaims, UserClaims) → `crates/common/src/jwt.rs`
- Token refresh event struct → `crates/common/src/token_manager.rs`
- GC error types → `crates/gc-service/src/errors.rs`
- MC error types → `crates/mc-service/src/errors.rs`
- GC metrics → `crates/gc-service/src/observability/metrics.rs`
- MC metrics → `crates/mc-service/src/observability/metrics.rs`

## Authentication Seams
- GC service JWT validation → `crates/gc-service/src/auth/jwt.rs:validate()`
- GC user JWT validation → `crates/gc-service/src/auth/jwt.rs:validate_user()`
- GC generic token verifier → `crates/gc-service/src/auth/jwt.rs:verify_token()`
- GC service auth middleware → `crates/gc-service/src/middleware/auth.rs:require_auth()`
- GC user auth middleware → `crates/gc-service/src/middleware/auth.rs:require_user_auth()`

## GC Meeting Creation
- Create meeting handler → `crates/gc-service/src/handlers/meetings.rs:create_meeting()`
- Meetings repository (CTE query) → `crates/gc-service/src/repositories/meetings.rs:MeetingsRepository`
- Meeting create models → `crates/gc-service/src/models/mod.rs:CreateMeetingRequest`
- Route wiring (user auth layer) → `crates/gc-service/src/routes/mod.rs:build_routes()`
- Meeting creation metrics → `crates/gc-service/src/observability/metrics.rs:record_meeting_creation()`
- Meeting creation alert rules → `infra/docker/prometheus/rules/gc-alerts.yaml`
- Incident response: limit exhaustion → `docs/runbooks/gc-incident-response.md` (Scenario 8)
- Incident response: code collision → `docs/runbooks/gc-incident-response.md` (Scenario 9)
- Deployment smoke test & monitoring checklist → `docs/runbooks/gc-deployment.md` (Test 6, Post-Deploy Monitoring)
