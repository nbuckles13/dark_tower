# DRY Reviewer - Cross-Service Integration Notes

## Known Duplication Patterns

### Tech Debt Registry

#### TD-1: JWT Signing Duplication
- **Location**: ac-service JWT signing + user-provisioning JWT signing
- **Pattern**: Both services implement JWT claim signing with EdDSA
- **Severity**: Medium (security code, but isolated per service)
- **Status**: DOCUMENTED, KNOWN
- **Improvement Path**: Extract to `common::crypto::jwt` utilities module
- **Timeline**: Phase 5+ (post-Phase 4 hardening)
- **Notes**: Each service maintains its own key management, so extraction must preserve that property

#### TD-2: Key Loading from Environment
- **Location**: ac-service key loading + global-controller key loading
- **Pattern**: Both load EdDSA keys from environment with SecretString protection
- **Severity**: Medium (security-critical, but straightforward extraction)
- **Status**: DOCUMENTED, KNOWN
- **Improvement Path**: Extract to `common::crypto::key_management::load_from_env()`
- **Timeline**: Phase 5+ (post-Phase 4 hardening)
- **Notes**: Must preserve SecretString semantics throughout extraction

## Integration Guidelines

### Working with Other Specialists

#### Security Specialist Handoff
- Always escalate duplication in cryptographic code to Security specialist
- Security may accept duplication if it reduces coupling
- Document security rationale in integration.md
- Reference ADR-0019 compliance

#### Code Reviewer Handoff
- Code Quality specialist focuses on style and patterns
- Coordinate if duplication involves architectural patterns
- DRY focuses on actual duplication; Code Quality focuses on structure
- Share findings if both identify same duplication

#### Test Specialist Coordination
- Duplication in test utilities may warrant extraction
- Test specialist has final say on test code organization
- Document test duplication separately in `gotchas.md`

### Cross-Service Review Process

**When reviewing code that touches multiple services:**

1. **Identify scope**: Which services does this code affect?
2. **Check history**: Is there established code in those services?
3. **Compare patterns**: Gather all implementations of the pattern
4. **Classify**: BLOCKER (new) vs TECH_DEBT (existing)
5. **Document**: Add to tech debt registry if TECH_DEBT
6. **Escalate**: Talk to specialist if security-related

## Architecture Considerations

### Service Boundaries and Duplication

**Principle**: Service boundaries take precedence over DRY

- Each service owns its implementation within domain
- Shared code must cross clear architectural lines
- Don't force cross-service coupling to eliminate duplication
- Use `common` crate strategically (with Security/Architecture oversight)

### Acceptable Duplication Patterns

These patterns are acceptable and should NOT be marked as issues:

1. **Per-service configuration loading** - Each service has different env vars
2. **Service-specific error types** - Defined in each service's error.rs
3. **Protocol message handling** - Each service may interpret messages differently
4. **Logging/metrics initialization** - Boilerplate is expected

### Duplication Requiring Extraction

These patterns should be extracted to `common`:

1. **Cryptographic operations** - Single source of truth required
2. **Standard library patterns** - Proven utilities should be shared
3. **3+ services repeating** - Extraction ROI is high
4. **Security-critical code** - Audit burden reduced by centralization

## Tracking and Future Work

### How Duplication Becomes Tech Debt

1. DRY reviewer identifies during code review
2. Classified as TECH_DEBT per ADR-0019
3. Added to tech debt registry (this file) with TD-N ID
4. Improvement path documented
5. Referenced in `.claude/TODO.md` or phase planning
6. Extracted in future phase when timeline permits

### Phase 5+ Refactoring Plans

Expected tech debt extraction work:
- **TD-1 Extraction**: Common JWT utilities (estimated 2 days)
- **TD-2 Extraction**: Common key management utilities (estimated 2 days)
- May uncover TD-3, TD-4, etc. during Phase 5

## Escalation Criteria

Escalate to Architecture specialist if:
- Duplication spans 3+ services
- Extraction requires database schema changes
- Pattern impacts protocol or API contracts
- Uncertainty about service boundary placement

Escalate to Security specialist if:
- Duplication involves cryptography
- Duplication involves authentication/authorization
- Pattern impacts threat model

