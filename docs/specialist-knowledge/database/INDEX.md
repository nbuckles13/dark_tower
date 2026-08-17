# Database Navigation

## Architecture & Design
- Service authentication schema → ADR-0003 (`adr-0003-service-authentication.md`)
- Key rotation strategy → ADR-0008 (`adr-0008-key-rotation-strategy.md`)
- Integration test infrastructure → ADR-0009 (`adr-0009-integration-test-infrastructure.md`)
- User auth & meeting access (new tables) → ADR-0020 (`adr-0020-user-auth-meeting-access.md`)
- Cluster access boundary for DB-touching scripts → ADR-0030 (`adr-0030-host-side-cluster-helper.md`)
- Data model reference (spec mirror of the schema) → `docs/DATABASE_SCHEMA.md`
- Schema details → see migration files below

## Migrations
- All migrations (chronological) → `migrations/`
- Participant tracking (ALTER TABLE) → `migrations/20260322000001_add_participant_tracking.sql`

## Code Locations — AC Service
- Repository modules → `crates/ac-service/src/repositories/mod.rs`
- Service credentials CRUD → `crates/ac-service/src/repositories/service_credentials.rs`
- Signing keys (rotation, JWKS) → `crates/ac-service/src/repositories/signing_keys.rs`
- Auth event logging/queries → `crates/ac-service/src/repositories/auth_events.rs`
- User lookups & roles → `crates/ac-service/src/repositories/users.rs`
- Organization lookup (fails closed on missing/inactive) → `crates/ac-service/src/repositories/organizations.rs:get_by_subdomain()`
- DB model structs (FromRow) → `crates/ac-service/src/models/mod.rs`
- Key management service → `crates/ac-service/src/services/key_management_service.rs`

## Code Locations — GC Service
- Repository modules → `crates/gc-service/src/repositories/mod.rs`
- MC registration & health → `crates/gc-service/src/repositories/meeting_controllers.rs`
- MH registration & load → `crates/gc-service/src/repositories/media_handlers.rs`
- Meeting assignments (weighted round-robin) → `crates/gc-service/src/repositories/meeting_assignments.rs`
- Meeting activation (scheduled→active) → `crates/gc-service/src/repositories/meetings.rs:activate_meeting()`
- Meeting audit logging (parameterized) → `crates/gc-service/src/repositories/meetings.rs:log_audit_event()`
- Participant tracking & capacity → `crates/gc-service/src/repositories/participants.rs`
- DB model structs (MeetingRow, Participant) → `crates/gc-service/src/models/mod.rs`

## Meeting Creation — Refusal Causes
- Creation CTE, always returns one diagnostic row → `crates/gc-service/src/repositories/meetings.rs:create_meeting_with_limit_check()`
- Outcome type (Created / Refused / MeetingCodeTaken) → `crates/gc-service/src/repositories/meetings.rs:CreateMeetingOutcome`
- Refusal cause enum + metric-label round-trip → `crates/gc-service/src/repositories/meetings.rs:MeetingRefusal`
- Unique-violation classification by SQLSTATE + constraint → `crates/gc-service/src/repositories/meetings.rs:classify_insert_error()`
- Refusal → HTTP status / error code → `crates/gc-service/src/errors.rs` (`impl From<MeetingRefusal> for GcError`)
- Refusal handling & per-cause metric labels → `crates/gc-service/src/handlers/meetings.rs`
- Wire contract for each cause → `docs/API_CONTRACTS.md`

## Cluster Data Provisioning
- Per-run organization row → `infra/kind/scripts/setup.sh:provision_run_org()` (`--provision-org` mode)
- psql invocation wrapper → `infra/kind/scripts/setup.sh:dt_psql()`
- Baseline `devtest` seed → `infra/kind/scripts/setup.sh:seed_test_data()`
- Concurrent-meeting cap constant → `infra/kind/scripts/setup.sh` (`ORG_MAX_CONCURRENT_MEETINGS`)
- Layer-7 provisioning call site (Phase 1h) → `scripts/layer7.sh:__generate_org_subdomain()`
- Org-subdomain pattern drift guard (7 encodings incl. `docs/DATABASE_SCHEMA.md`) → `scripts/guards/simple/validate-subdomain-regex-sync.sh`

## Integration Seams
- Test DB harness (PgPool fixture, AC) → `crates/ac-test-utils/src/server_harness.rs`
- Test DB harness (PgPool fixture, GC) → `crates/gc-test-utils/src/server_harness.rs`
- Docker Compose (test DB) → `docker-compose.test.yml`
- Auth middleware (reads DB via services) → `crates/ac-service/src/middleware/auth.rs`
- Org extraction middleware → `crates/ac-service/src/middleware/org_extraction.rs`
- Env-test org subdomain resolution → `crates/env-tests/src/fixtures/auth_client.rs:resolve_org_subdomain()`
- Browser-suite org subdomain → `packages/web-app/e2e/env.ts`
- Meeting-creation & refusal tests → `crates/gc-service/tests/meeting_create_tests.rs`
- Participant & activation integration tests → `crates/gc-service/tests/participant_tests.rs`
- Provisioning script tests → `scripts/setup.test.sh`
- GuestTokenClaims validation → `crates/common/src/jwt.rs:GuestTokenClaims::validate()`
