# Semantic Guard Navigation

## Architecture & Design
- Guard methodology & principles → ADR-0015 (`docs/decisions/adr-0015-principles-guards-methodology.md`)
- Agent Teams validation pipeline → ADR-0024 (`docs/decisions/adr-0024-agent-teams-workflow.md`)

## Guard Code
- Semantic check definitions → `scripts/guards/semantic/checks.md`
- Shared guard utilities → `scripts/guards/common.sh`

## Metrics Catalogs (Label Validation)
- AC metrics catalog → `docs/observability/metrics/ac-service.md`
- GC metrics catalog → `docs/observability/metrics/gc-service.md`
- MC metrics catalog → `docs/observability/metrics/mc-service.md`

## Cross-Service Boundary Files
- Shared JWT types (ServiceClaims, UserClaims, MeetingTokenClaims, GuestTokenClaims) → `crates/common/src/jwt.rs`
- Meeting token enums & guest validation → `crates/common/src/jwt.rs:ParticipantType`, `MeetingRole`, `GuestTokenClaims::validate()`
- Token refresh → `crates/common/src/token_manager.rs`
- GC error types → `crates/gc-service/src/errors.rs`
- MC error types → `crates/mc-service/src/errors.rs`

## Authentication Seams
- GC JWT validation → `crates/gc-service/src/auth/jwt.rs:validate()`, `validate_user()`, `verify_token()`
- GC auth middleware → `crates/gc-service/src/middleware/auth.rs:require_auth()`, `require_user_auth()`

## GC Meeting Handlers
- Create/Join/Guest/Settings handlers → `crates/gc-service/src/handlers/meetings.rs`
- Route wiring (user auth layer) → `crates/gc-service/src/routes/mod.rs:build_routes()`

## GC Repositories
- Meetings repository (CTE, activation, audit) → `crates/gc-service/src/repositories/meetings.rs`
- Participants repository → `crates/gc-service/src/repositories/participants.rs:ParticipantsRepository`
- Participant tracking migration → `migrations/20260322000001_add_participant_tracking.sql`

## GC Models
- Meeting models → `crates/gc-service/src/models/mod.rs:CreateMeetingRequest`, `Participant`

## GC Metrics & Observability
- Meeting creation metrics → `crates/gc-service/src/observability/metrics.rs:record_meeting_creation()`
- Meeting join metrics → `crates/gc-service/src/observability/metrics.rs:record_meeting_join()`
- GC metrics impl → `crates/gc-service/src/observability/metrics.rs`
- MC metrics impl → `crates/mc-service/src/observability/metrics.rs`
- Alert rules → `infra/docker/prometheus/rules/gc-alerts.yaml`

## GC Runbooks
- Incident response: limit exhaustion → `docs/runbooks/gc-incident-response.md` (Scenario 8)
- Incident response: code collision → `docs/runbooks/gc-incident-response.md` (Scenario 9)
- Deployment checklist → `docs/runbooks/gc-deployment.md` (Test 6, Post-Deploy Monitoring)
