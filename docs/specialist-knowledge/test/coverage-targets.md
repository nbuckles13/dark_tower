# Test Coverage Targets

Last reviewed: 2026-02-10

## Coverage by Code Type

| Code Type | Target | Rationale |
|-----------|--------|-----------|
| Authentication/Authorization | 95%+ | Security critical |
| Cryptographic operations | 100% | Security critical (enforced by .codecov.yml) |
| Data persistence | 90%+ | Data integrity |
| Public APIs | 90%+ | Contract stability |
| Business logic | 85%+ | Core functionality |
| Error handling | 80%+ | Reliability |
| Utilities | 70%+ | Supporting code |

## Coverage by Component (from .codecov.yml)

| Component | Path Pattern | Target |
|-----------|--------------|--------|
| Cryptography | `**/crypto/**` | 100% |
| Auth Controller | `crates/ac-service/src/{handlers,services,repositories}/**` | 95% |
| Middleware | `**/middleware/**` | 90% |
| Data Models | `**/models/**` | 85% |
| Project default | All other code | 90% |
| Patch (new code) | Changed lines in PR | 95% |

## Excluded from Coverage

The following are excluded in `.codecov.yml` and should not be flagged for low coverage:

| Path | Reason |
|------|--------|
| `crates/proto-gen/**` | Generated protobuf code |
| `crates/env-tests/**` | Test infrastructure (runs against live cluster) |
| `**/benches/**` | Benchmark code |
| `**/examples/**` | Example code |
| `**/.claude/**` | Agent configurations |

## Coverage by Service (Current State)

| Service | Current | Target | Notes |
|---------|---------|--------|-------|
| ac-service | 83% | 95% | Security critical, active improvement |
| global-controller | TBD | 90% | API gateway |
| meeting-controller | TBD | 90% | Real-time critical |
| media-handler | TBD | 85% | Performance critical |

## Critical Paths (Require Complete Coverage)

These paths must have thorough test coverage:
- User authentication flow
- Service authentication flow (OAuth 2.0 client credentials)
- JWT validation (signature, expiration, claims)
- Token issuance
- Key rotation
- Meeting creation and join

## Test Types Required

| Type | Purpose | Where |
|------|---------|-------|
| Unit tests | Business logic, utilities | `#[cfg(test)] mod tests` |
| Integration tests | Database, service calls | `tests/` directory |
| E2E tests | Full user journeys | `crates/env-tests/` |
| Security tests | Attack vectors, crypto | P0/P1 priority tests |
| Fuzz tests | Input validation | `fuzz/` directory |

## Measurement

- **Tool**: `cargo llvm-cov`
- **CI enforcement**: Yes, via Codecov
- **Report format**: `lcov.info` generated on each PR
- **Configuration**: `.codecov.yml` in repo root

## Updating Targets

When updating coverage targets:
1. Update `.codecov.yml` for CI enforcement
2. Update this file for documentation
3. Consider adding component-specific thresholds for new services
