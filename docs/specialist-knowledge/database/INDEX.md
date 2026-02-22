# Database Navigation

## Architecture & Design
- Service authentication schema → ADR-0003 (`adr-0003-service-authentication.md`)
- Key rotation strategy → ADR-0008 (`adr-0008-key-rotation-strategy.md`)
- Integration test infrastructure → ADR-0009 (`adr-0009-integration-test-infrastructure.md`)
- User auth & meeting access (new tables) → ADR-0020 (`adr-0020-user-auth-meeting-access.md`)
- Schema details → see migration files below

## Migrations
- All migrations (chronological) → `migrations/`

## Code Locations — AC Service
- Repository modules → `crates/ac-service/src/repositories/mod.rs`
- Service credentials CRUD → `crates/ac-service/src/repositories/service_credentials.rs`
- Signing keys (rotation, JWKS) → `crates/ac-service/src/repositories/signing_keys.rs`
- Auth event logging/queries → `crates/ac-service/src/repositories/auth_events.rs`
- User lookups & roles → `crates/ac-service/src/repositories/users.rs`
- Organization lookups → `crates/ac-service/src/repositories/organizations.rs`
- DB model structs (FromRow) → `crates/ac-service/src/models/mod.rs`
- Key management service → `crates/ac-service/src/services/key_management_service.rs`

## Code Locations — GC Service
- Repository modules → `crates/gc-service/src/repositories/mod.rs`
- MC registration & health → `crates/gc-service/src/repositories/meeting_controllers.rs`
- MH registration & load → `crates/gc-service/src/repositories/media_handlers.rs`
- Meeting assignments (weighted round-robin) → `crates/gc-service/src/repositories/meeting_assignments.rs`
- DB model structs → `crates/gc-service/src/models/mod.rs`

## Integration Seams
- Test DB harness (PgPool fixture) → `crates/ac-test-utils/src/server_harness.rs`
- Docker Compose (test DB) → `docker-compose.test.yml`
- Auth middleware (reads DB via services) → `crates/ac-service/src/middleware/auth.rs`
- Org extraction middleware → `crates/ac-service/src/middleware/org_extraction.rs`
