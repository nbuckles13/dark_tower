# Database Navigation

## Architecture & Design
- Service auth schema; key rotation → ADR-0003, ADR-0008
- Integration test infrastructure → ADR-0009 (`adr-0009-integration-test-infrastructure.md`)
- User auth & meeting access (new tables) → ADR-0020 (`adr-0020-user-auth-meeting-access.md`)
- GC registry & assignment tables (meeting_assignments, media_handlers) → ADR-0010 (`adr-0010-global-controller-architecture.md`)
- MH registry rules, replacement, no-migration constraint → ADR-0039 (`adr-0039-mh-replacement-and-migration.md`)
- Cluster access boundary for DB-touching scripts → ADR-0030 (`adr-0030-host-side-cluster-helper.md`)
- Data model reference (spec mirror of the schema) → `docs/DATABASE_SCHEMA.md`

## Migrations
- Participant tracking (ALTER TABLE) → `migrations/20260322000001_add_participant_tracking.sql`
- Assignment incarnation fence (`assignment_id`, roll-forward only) → `migrations/20261115000001_meeting_assignment_id.sql`
- Schema rollback procedure (GC) → `docs/runbooks/gc-deployment.md` §Database Rollback Considerations

## Code Locations — AC Service
- Service credentials CRUD → `crates/ac-service/src/repositories/service_credentials.rs`
- Signing keys (rotation, JWKS) → `crates/ac-service/src/repositories/signing_keys.rs`
- Auth event logging/queries → `crates/ac-service/src/repositories/auth_events.rs`
- User lookups & roles → `crates/ac-service/src/repositories/users.rs`
- Organization lookup (fails closed on missing/inactive) → `crates/ac-service/src/repositories/organizations.rs:get_by_subdomain()`
- DB model structs (FromRow) → `crates/ac-service/src/models/mod.rs`
- Key management service → `crates/ac-service/src/services/key_management_service.rs`

## Code Locations — GC Service
- MC registration & health → `crates/gc-service/src/repositories/meeting_controllers.rs`
- Meeting activation; audit logging → `crates/gc-service/src/repositories/meetings.rs:activate_meeting()`, `log_audit_event()`
- Participant tracking & capacity → `crates/gc-service/src/repositories/participants.rs`
- DB model structs (MeetingRow, Participant) → `crates/gc-service/src/models/mod.rs`

## Meeting Assignments (liveness-keyed, incarnation-fenced)
- Join assign / revive / stale-MC re-point → `crates/gc-service/src/repositories/meeting_assignments.rs:atomic_assign()`
- Fenced end (controller id + assignment_id) → `crates/gc-service/src/repositories/meeting_assignments.rs:end_assignment()`
- Backstop for rows whose MC stopped heartbeating → `crates/gc-service/src/repositories/meeting_assignments.rs:end_stale_assignments()`
- Cleanup task & its config → `crates/gc-service/src/tasks/assignment_cleanup.rs`, `crates/gc-service/src/config.rs:AssignmentCleanupConfig`
- Staleness threshold / commit budget → `crates/gc-service/src/config.rs:McAssignmentConfig`

## MH Registry
- Atomic registration (supersede/reject decision) → `crates/gc-service/src/repositories/media_handlers.rs:register_mh()`
- Load report (never recreates a deleted row) → `crates/gc-service/src/repositories/media_handlers.rs:update_load_report()`
- Stale sweep & retention delete → `crates/gc-service/src/repositories/media_handlers.rs:mark_stale_handlers_unhealthy()`, `delete_expired_handlers()`
- Selection candidates (healthy + fresh only) → `crates/gc-service/src/repositories/media_handlers.rs:get_candidate_mhs()`
- Registry tick; staleness/retention config → `crates/gc-service/src/tasks/mh_health_checker.rs:mh_registry_tick()`, `crates/gc-service/src/config.rs:MhRegistryConfig`
- Replacement acquire (reads registry row) → `crates/gc-service/src/services/mh_replacement.rs:acquire()`
- Weighted pick over candidate rows → `crates/gc-service/src/services/weighted_pick.rs:weighted_random_select()`

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
- psql invocation wrapper → `infra/kind/scripts/lib/cluster-db.sh:dt_psql()`
- Baseline `devtest` seed → `infra/kind/scripts/deploy.sh:seed_test_data()`
- Concurrent-meeting cap constant → `infra/kind/scripts/lib/cluster-db.sh` (`ORG_MAX_CONCURRENT_MEETINGS`)
- Layer-7 provisioning call site (Phase 1h) → `scripts/layer7.sh:__generate_org_subdomain()`
- Org-subdomain pattern drift guard (7 encodings incl. `docs/DATABASE_SCHEMA.md`) → `scripts/guards/simple/validate-subdomain-regex-sync.sh`

## Integration Seams
- Test DB harness (PgPool fixture) → `crates/ac-test-utils/src/server_harness.rs`, `crates/gc-test-utils/src/server_harness.rs`
- Docker Compose (test DB) → `docker-compose.test.yml`
- AC auth / org-extraction middleware → `crates/ac-service/src/middleware/auth.rs`, `crates/ac-service/src/middleware/org_extraction.rs`
- Org subdomain resolution (env-tests; browser) → `crates/env-tests/src/fixtures/auth_client.rs:resolve_org_subdomain()`, `packages/web-app/e2e/env.ts`
- Meeting-creation & refusal tests → `crates/gc-service/tests/meeting_create_tests.rs`
- Participant & activation integration tests → `crates/gc-service/tests/participant_tests.rs`
- Assignment & fenced-end tests → `crates/gc-service/tests/meeting_assignment_tests.rs`, `crates/gc-service/tests/meeting_end_notify_tests.rs`
- MH registry/registration tests (+ fixture) → `crates/gc-service/tests/mh_registration_tests.rs`, `crates/gc-service/tests/common/mh_registry.rs`
- Provisioning script tests → `scripts/setup.test.sh`
- GuestTokenClaims validation → `crates/common/src/jwt.rs:GuestTokenClaims::validate()`
