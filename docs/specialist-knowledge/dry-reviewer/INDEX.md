# DRY Reviewer Navigation

## Architecture & Design
- Blocking vs tech-debt classification -> ADR-0019 (`docs/decisions/adr-0019-dry-reviewer.md`)
- Fix-or-defer review model -> ADR-0024 (`docs/decisions/adr-0024-agent-teams-workflow.md`)

## Shared Code (Duplication Prevention)
- Common crate modules -> `crates/common/src/lib.rs`
- JWT utilities (extracted TD-1/TD-2) -> `crates/common/src/jwt.rs:extract_kid()`
- ServiceClaims (extracted from AC) -> `crates/common/src/jwt.rs:ServiceClaims`
- UserClaims (extracted from AC, ADR-0020) -> `crates/common/src/jwt.rs:UserClaims`
- Token management (prevents static token dup) -> `crates/common/src/token_manager.rs:TokenManagerConfig`
- Secret types -> `crates/common/src/secret.rs`
- Domain IDs and shared types -> `crates/common/src/types.rs`

## Tech Debt Registry
- Active duplication tech debt → `.claude/TODO.md` (Cross-Service Duplication section)

## Successful Extractions (Reference)
- ServiceClaims to common::jwt (AC re-exports as `Claims`) -> `crates/ac-service/src/crypto/mod.rs:23`
- UserClaims to common::jwt (AC re-exports) -> `crates/ac-service/src/crypto/mod.rs:29`
- GC uses UserClaims from common::jwt (not reimplemented) -> `crates/gc-service/src/handlers/meetings.rs`, `crates/gc-service/src/auth/jwt.rs`
- Generic verify_token<T> (service + user tokens, single JWK path) -> `crates/gc-service/src/auth/jwt.rs:verify_token()`
- Shared bearer token extraction (both auth middlewares) -> `crates/gc-service/src/middleware/auth.rs:extract_bearer_token()`
- Shared map_row_to_meeting (handler + repo, single definition) -> `crates/gc-service/src/repositories/meetings.rs:map_row_to_meeting()`
- Closure-based generic extraction -> `crates/gc-service/src/tasks/generic_health_checker.rs:start_generic_health_checker()`
- Thin wrappers after extraction -> `crates/gc-service/src/tasks/health_checker.rs`, `crates/gc-service/src/tasks/mh_health_checker.rs`

## False Positive Boundaries
- Actor vs controller metrics (different consumers) -> `crates/mc-service/src/actors/metrics.rs`
- Service-prefixed metric names (convention) -> per-service `observability/metrics.rs`
- Per-subsystem metric functions (record_meeting_creation vs record_token_refresh) -> per-service `observability/metrics.rs`

## K8s Manifest Patterns
- NetworkPolicy cross-refs -> `infra/services/{ac,gc,mc}-service/network-policy.yaml`
- ServiceMonitor cross-refs -> `infra/services/{ac,gc,mc}-service/service-monitor.yaml`
- GC ServiceMonitor (first enabled, reference pattern) -> `infra/services/gc-service/service-monitor.yaml`

## Integration Seams
- Common crate as extraction target -> `crates/common/src/`
- GC repositories (shared row mappers) -> `crates/gc-service/src/repositories/`
- GC dual auth middleware (service + user) -> `crates/gc-service/src/middleware/auth.rs`
- NetworkPolicy egress/ingress pairs (GC<->MC on 50052, GC<->AC on 8082, MC->AC on 8082) -> `infra/services/*/network-policy.yaml`
