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
- Token refresh event struct → `crates/common/src/token_manager.rs`
- GC error types → `crates/gc-service/src/errors.rs`
- MC error types → `crates/mc-service/src/errors.rs`
- GC metrics → `crates/gc-service/src/observability/metrics.rs`
- MC metrics → `crates/mc-service/src/observability/metrics.rs`
