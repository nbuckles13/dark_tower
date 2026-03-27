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
- Common JWKS client & JWT validator → `crates/common/src/jwt.rs:JwksClient`, `JwtValidator`, `verify_token()`
- JwtError enum (7 variants) → `crates/common/src/jwt.rs:JwtError`
- HasIat trait (compile-time iat enforcement) → `crates/common/src/jwt.rs:HasIat`
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
- Meeting creation metrics → `crates/gc-service/src/observability/metrics.rs:record_meeting_creation()`
- Meeting join metrics → `crates/gc-service/src/observability/metrics.rs:record_meeting_join()`
- GC metrics impl → `crates/gc-service/src/observability/metrics.rs`
- GC alert rules → `infra/docker/prometheus/rules/gc-alerts.yaml`

## MC Metrics & Observability
- MC metrics impl → `crates/mc-service/src/observability/metrics.rs`
- WebTransport connection counter → `crates/mc-service/src/observability/metrics.rs:record_webtransport_connection()`
- JWT validation counter → `crates/mc-service/src/observability/metrics.rs:record_jwt_validation()`
- Session join counter + histogram → `crates/mc-service/src/observability/metrics.rs:record_session_join()`
- MC alert rules → `infra/docker/prometheus/rules/mc-alerts.yaml`

## GC Runbooks
- Incident response: limit exhaustion → `docs/runbooks/gc-incident-response.md` (Scenario 8)
- Incident response: code collision → `docs/runbooks/gc-incident-response.md` (Scenario 9)
- Deployment checklist → `docs/runbooks/gc-deployment.md` (Test 6, Post-Deploy Monitoring)
