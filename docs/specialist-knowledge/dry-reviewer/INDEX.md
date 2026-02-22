# DRY Reviewer Navigation

## Architecture & Design
- Blocking vs tech-debt classification -> ADR-0019 (`docs/decisions/adr-0019-dry-reviewer.md`)
- Fix-or-defer review model -> ADR-0024 (`docs/decisions/adr-0024-agent-teams-workflow.md`)

## Shared Code (Duplication Prevention)
- Common crate modules -> `crates/common/src/lib.rs`
- JWT utilities (extracted TD-1/TD-2) -> `crates/common/src/jwt.rs:extract_kid()`
- Token management (prevents static token dup) -> `crates/common/src/token_manager.rs:TokenManagerConfig`
- Secret types -> `crates/common/src/secret.rs`
- Domain IDs and shared types -> `crates/common/src/types.rs`

## Tech Debt Registry
- Active duplication tech debt → `.claude/TODO.md` (Cross-Service Duplication section)

## Successful Extractions (Reference)
- Closure-based generic extraction -> `crates/gc-service/src/tasks/generic_health_checker.rs:start_generic_health_checker()`
- Thin wrappers after extraction -> `crates/gc-service/src/tasks/health_checker.rs`, `crates/gc-service/src/tasks/mh_health_checker.rs`

## False Positive Boundaries
- Actor vs controller metrics (different consumers) -> `crates/mc-service/src/actors/metrics.rs`
- Service-prefixed metric names (convention) -> per-service `observability/metrics.rs`

## Integration Seams
- Common crate as extraction target -> `crates/common/src/`
