# DRY Reviewer - Patterns That Work

## Successful Approaches

### 1. ADR-0019 Duplication Classification
**Pattern**: Distinguish between BLOCKER duplication and TECH_DEBT duplication
- **BLOCKER**: New duplication introduced that violates established patterns
- **TECH_DEBT**: Existing duplication patterns already documented in codebase
- **When to apply**: All code reviews where duplication is detected

**Why it works**: Allows progress on features without requiring all tech debt resolved first, while maintaining architectural integrity

### 2. Cross-Service Pattern Registry
**Pattern**: Document established duplication patterns across services in integration.md
- **Content**: List known duplication patterns (JWT signing, key loading, etc.)
- **Reference ID**: Assign tech debt IDs (TD-1, TD-2, etc.) for tracking
- **When to apply**: During knowledge accumulation, before future reviews

**Why it works**: Enables quick classification of similar issues in future reviews, prevents repeated discovery

### 3. Duplication Severity Tiers
**Pattern**: Use tiers to assess duplication impact:
- **Tier 1 (BLOCKER)**: Critical security implications, breaks DRY principle, new pattern
- **Tier 2 (TECH_DEBT)**: Known pattern, documented, improvement tracked
- **Tier 3 (ACCEPTABLE)**: Small isolated code, cost of extraction > benefit

**Why it works**: Focuses energy on highest-impact duplication while acknowledging technical realities

## Code Review Integration

### Successful Review Flow
1. Identify all duplication points in changeset
2. Check against cross-service pattern registry (integration.md)
3. Classify as BLOCKER or TECH_DEBT per ADR-0019
4. Document findings with reference IDs
5. Approve/reject based on blocking status only

### Effective Documentation
- Always cite ADR-0019 when applying TECH_DEBT classification
- Reference existing tech debt IDs when applicable (TD-1, TD-2, etc.)
- Include rationale for BLOCKER classification
- Provide clear improvement path for TECH_DEBT items

