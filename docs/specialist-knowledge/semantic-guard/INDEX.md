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
- Common JWT (types, JWKS, validator, errors, HasIat) → `crates/common/src/jwt.rs`
- Token refresh → `crates/common/src/token_manager.rs`
- GC error types & JwtError mapping → `crates/gc-service/src/errors.rs`
- MC error types & JwtError mapping → `crates/mc-service/src/errors.rs`

## Authentication Seams
- GC JWT validation (thin wrapper) → `crates/gc-service/src/auth/jwt.rs:validate()`, `validate_user()`
- GC JWKS re-export → `crates/gc-service/src/auth/jwks.rs`
- GC auth middleware → `crates/gc-service/src/middleware/auth.rs:require_auth()`, `require_user_auth()`
- MC JWT validation (thin wrapper) → `crates/mc-service/src/auth/mod.rs:McJwtValidator`
- MC meeting token validation → `crates/mc-service/src/auth/mod.rs:validate_meeting_token()`, `validate_guest_token()`
- MC JWKS config → `crates/mc-service/src/config.rs:ac_jwks_url`
- MC WebTransport JWT check (pre-actor) → `crates/mc-service/src/webtransport/connection.rs:handle_connection()`

## MC Actor Hierarchy
- Controller actor (root supervisor) → `crates/mc-service/src/actors/controller.rs:MeetingControllerActor`
- Meeting actor (per-meeting state) → `crates/mc-service/src/actors/meeting.rs:MeetingActor`
- Participant actor (per-participant, disconnect notify) → `crates/mc-service/src/actors/participant.rs:ParticipantActor`
- Actor messages (ControllerMessage, MeetingMessage, ParticipantMessage) → `crates/mc-service/src/actors/messages.rs`
- Actor metrics (ActorType::Participant) → `crates/mc-service/src/actors/metrics.rs`

## MC WebTransport Layer
- Server (accept loop, TLS, capacity gate) → `crates/mc-service/src/webtransport/server.rs:WebTransportServer`
- Connection handler (join flow, bridge loop) → `crates/mc-service/src/webtransport/connection.rs:handle_connection()`
- Protobuf encoding utilities → `crates/mc-service/src/webtransport/handler.rs:encode_participant_update()`

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
- GC metrics impl → `crates/gc-service/src/observability/metrics.rs`
- GC dashboard (join gauge panel 38, join rate/latency/failures) → `infra/grafana/dashboards/gc-overview.json`
- GC alerts (critical/warning/info tiers) → `infra/docker/prometheus/rules/gc-alerts.yaml`
- Join alerts: `GCHighJoinFailureRate` (warning), `GCHighJoinLatency` (info)
- GC metrics catalog (dashboard/alert cross-refs) → `docs/observability/metrics/gc-service.md`

## MC Metrics & Observability
- MC metrics impl → `crates/mc-service/src/observability/metrics.rs`
- MC alert rules (critical/warning/info tiers) → `infra/docker/prometheus/rules/mc-alerts.yaml`
- Join alerts: `MCHighJoinFailureRate` (warning), `MCHighWebTransportRejections` (warning), `MCHighJwtValidationFailures` (warning), `MCHighJoinLatency` (info)
- MC dashboard (overview + join flow row) → `infra/grafana/dashboards/mc-overview.json`
- MC metrics catalog (alert/dashboard cross-refs) → `docs/observability/metrics/mc-service.md`

## Observability Docs
- Alerts docs → `docs/observability/alerts.md` | Dashboards docs → `docs/observability/dashboards.md`

## GC Runbooks
- Incident response: MC assignment failures → `docs/runbooks/gc-incident-response.md` (Scenario 3)
- Incident response: limit exhaustion → `docs/runbooks/gc-incident-response.md` (Scenario 8)
- Deployment checklist → `docs/runbooks/gc-deployment.md`
