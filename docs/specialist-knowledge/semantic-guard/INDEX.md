# Semantic Guard Navigation

## Architecture & Design
- Guard methodology & principles → ADR-0015 (`docs/decisions/adr-0015-principles-guards-methodology.md`)
- Agent Teams validation pipeline → ADR-0024 (`docs/decisions/adr-0024-agent-teams-workflow.md`)

## Guard Code
- Semantic check definitions → `scripts/guards/semantic/checks.md`
- Shared guard utilities → `scripts/guards/common.sh`

## Metrics Catalogs (Label Validation)
- AC → `docs/observability/metrics/ac-service.md` | GC → `docs/observability/metrics/gc-service.md` | MC → `docs/observability/metrics/mc-service.md`

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
- MC meeting/guest token validation → `crates/mc-service/src/auth/mod.rs:validate_meeting_token()`, `validate_guest_token()`
- MC JWKS config → `crates/mc-service/src/config.rs:ac_jwks_url`
- MC WebTransport JWT check (pre-actor) → `crates/mc-service/src/webtransport/connection.rs:handle_connection()`

## MC Actor Hierarchy
- Controller actor → `crates/mc-service/src/actors/controller.rs:MeetingControllerActor`
- Meeting actor → `crates/mc-service/src/actors/meeting.rs:MeetingActor`
- Participant actor → `crates/mc-service/src/actors/participant.rs:ParticipantActor`
- Messages → `crates/mc-service/src/actors/messages.rs` | Metrics → `crates/mc-service/src/actors/metrics.rs`

## MC WebTransport Layer
- Server (accept loop, TLS, capacity gate) → `crates/mc-service/src/webtransport/server.rs:WebTransportServer`
- Connection handler (join flow, bridge loop) → `crates/mc-service/src/webtransport/connection.rs:handle_connection()`
- Protobuf encoding utilities → `crates/mc-service/src/webtransport/handler.rs:encode_participant_update()`

## GC Meeting Handlers & Routes
- Create/Join/Guest/Settings handlers → `crates/gc-service/src/handlers/meetings.rs`
- Route wiring (user auth layer, Result<Router>) → `crates/gc-service/src/routes/mod.rs:build_routes()`

## GC Repositories & Models
- Meetings repo (CTE, activation, audit) → `crates/gc-service/src/repositories/meetings.rs`
- Participants repo → `crates/gc-service/src/repositories/participants.rs:ParticipantsRepository`
- Meeting models → `crates/gc-service/src/models/mod.rs:CreateMeetingRequest`, `Participant`

## GC Join Integration Tests (`crates/gc-service/tests/meeting_tests.rs`)
- Test harness: TestMeetingServer (spawn/spawn_with_ac_failure), wiremock JWKS+AC, MockMcClient, `#[sqlx::test]`
- Join success: scheduled + active status, host + non-host member, cross-org allowed, non-existent user w/ valid org_id
- Join denied: not found, cancelled, ended, cross-org forbidden, missing/invalid/expired auth, service token on user endpoint
- Dependency failures: no MC available (503), AC unavailable (503)
- JWT security: algorithm confusion (HS256), wrong key, tampered payload → all return 401
- Guest token: success, not found, forbidden, display_name validation (empty/whitespace/short/long), captcha, concurrency (20 parallel)
- Settings update: host allow_guests/external/waiting_room, partial + multi-field, non-host 403, not found 404, empty 400, no auth 401

## GC Metrics & Observability
- GC metrics impl → `crates/gc-service/src/observability/metrics.rs`
- GC dashboard (join gauge panel 38, join rate/latency/failures) → `infra/grafana/dashboards/gc-overview.json`
- GC alerts (critical/warning/info tiers) → `infra/docker/prometheus/rules/gc-alerts.yaml`
- GC metrics catalog (dashboard/alert cross-refs) → `docs/observability/metrics/gc-service.md`

## MC Metrics & Observability
- MC metrics impl → `crates/mc-service/src/observability/metrics.rs`
- MC alert rules → `infra/docker/prometheus/rules/mc-alerts.yaml`
- MC dashboard (overview + join flow row) → `infra/grafana/dashboards/mc-overview.json`
- MC metrics catalog (alert/dashboard cross-refs) → `docs/observability/metrics/mc-service.md`

## Observability Docs
- Alerts → `docs/observability/alerts.md` | Dashboards → `docs/observability/dashboards.md`

## GC Runbooks
- Incident response: MC assignment (Scenario 3), limit exhaustion (Scenario 8) → `docs/runbooks/gc-incident-response.md`
- Deployment checklist → `docs/runbooks/gc-deployment.md`
