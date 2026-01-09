# Principle: API Design

**All HTTP APIs MUST use URL path versioning (`/api/v{N}/...`).** Support N and N-1 simultaneously, deprecate with 6-month sunset.

**ADRs**: ADR-0004 (API Versioning)

---

## DO

### URL Versioning
- **Include version in all API paths** - `/api/v1/meetings`, `/api/v1/auth/token`
- **Use integer versions** - v1, v2, v3 (not v1.1 or dates)
- **Support N and N-1** - when v2 releases, support both v1 and v2

### Breaking vs Non-Breaking Changes
- **Increment version for breaking changes** - remove endpoint, remove required field, change type
- **Keep same version for non-breaking** - add endpoint, add optional field, add enum value

### Deprecation Process
- **Add `Deprecation: true` header** when deprecating
- **Add `Sunset` header** with removal date
- **Wait 6 months minimum** before removal
- **Monitor usage** - remove only when usage < 1%

### Well-Known URIs (Exception)
- **Don't version RFC-defined paths** - `/.well-known/jwks.json`, `/.well-known/openid-configuration`
- **These are standards** - clients expect exact paths

### Protobuf Evolution (Internal APIs)
- **Use package versioning** - `package dark_tower.internal.v1;`
- **Add new fields safely** - old receivers ignore, new use defaults
- **Mark deprecated fields** - `[deprecated = true]`
- **Reserve removed field numbers** - `reserved 5, 6;`
- **Never reuse field numbers** - breaks wire compatibility

### Error Responses
- **Return supported versions on mismatch** - `"supported_versions": ["v1", "v2"]`
- **Include deprecation notice in body** - for deprecated versions

---

## DON'T

### Versioning
- **NEVER use header versioning** - less visible, cache-unfriendly
- **NEVER use query parameter versioning** - inconsistent, not RESTful
- **NEVER remove versions without deprecation period** - breaks clients

### Breaking Changes
- **NEVER remove required fields** without version bump
- **NEVER change field types** without version bump
- **NEVER change authentication methods** without version bump

### Protobuf
- **NEVER reuse deleted field numbers** - `reserved` instead
- **NEVER change field numbers** - breaks existing clients
- **NEVER remove enum values** - use reserved
- **NEVER use field numbers 19000-19999** - reserved by protobuf

---

## Quick Reference

### Breaking vs Non-Breaking

| Change | Version Impact |
|--------|----------------|
| Add new endpoint | None |
| Add optional field | None |
| Add new enum value | None |
| Bug fix | None |
| Remove endpoint | Major (v1→v2) |
| Remove required field | Major |
| Change field type | Major |
| Rename field | Major |

### Deprecation Timeline

| Event | Action |
|-------|--------|
| v2 released | Add `Deprecation: true` to v1 |
| +0 months | Both v1 and v2 supported |
| +6 months | Remove v1 if usage < 1% |

### Protobuf Field Numbers

| Range | Usage |
|-------|-------|
| 1-15 | High-frequency fields (1-byte encoding) |
| 16-2047 | Standard fields (2-byte encoding) |
| 19000-19999 | Reserved by protobuf (never use) |

### Standard Headers

| Header | Purpose |
|--------|---------|
| `Deprecation: true` | Mark version as deprecated |
| `Sunset: <date>` | Planned removal date |
| `Link: <url>; rel="successor-version"` | Point to new version |

---

## Guards

**`scripts/guards/simple/api-version-check.sh`** detects:
- Route definitions without /api/v{N}/ or /v{N}/ prefix
- Exceptions: /.well-known/*, /health, /ready, /metrics, /internal/*

**Code Review**: Verify version increments for breaking changes
**CI**: Validate protobuf field number usage (no reuse, no 19000-19999)
