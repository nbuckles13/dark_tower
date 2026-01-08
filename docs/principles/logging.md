# Principle: Logging Safety

**Sensitive data (passwords, tokens, secrets, PII) MUST NEVER appear in logs, traces, or error messages.** Privacy-by-default with explicit opt-in for safe fields.

**ADRs**: ADR-0011 (Observability), ADR-0002 (No-Panic)
**Guards**: `scripts/guards/simple/no-secrets-in-logs.sh`, `scripts/guards/semantic/credential-leak.sh`

---

## DO

### Instrumentation
- **Use `#[instrument(skip_all)]` by default** on handlers and critical functions
- **Explicitly allow-list SAFE fields** in `fields()` clause
- **Include correlation IDs** (`trace_id`, `request_id`) for debugging
- **Follow span naming convention** `{service}.{subsystem}.{operation}` (e.g., `ac.token.issue`)

### Sensitive Data
- **Wrap secrets in `SecretString`** - auto-redacts in Debug output as `Secret([REDACTED])`
- **Use `skip(password, secret, token, key)` in `#[instrument]`**
- **Log metadata, not values** - token expiration, scope count, not actual tokens

### Error Handling
- **Return generic errors to clients** - "invalid or expired", not specific failure reasons
- **Log detailed errors internally** with correlation IDs for debugging

### Visibility Levels
- **DEBUG**: Full payloads (dev only, never staging/prod)
- **INFO**: SAFE fields only
- **WARN/ERROR**: Correlation IDs and error classification only

---

## DON'T

### Credentials
- **NEVER log raw credentials** - passwords, secrets, tokens, API keys
- **NEVER use `#[instrument]` without `skip`** on functions with secret parameters
- **NEVER call `.expose_secret()` in log statements**
- **NEVER use `{:?}` on structs containing secrets** unless they use `SecretString`

### PII
- **NEVER log PII** - emails, phone numbers, real names, IP addresses (unless hashed)
- **NEVER log user agents** - fingerprinting risk

### Information Leakage
- **NEVER include secrets in error messages** - use generic messages
- **NEVER log request/response bodies** from auth endpoints
- **NEVER use high-cardinality span attributes** - hash or bound IDs

### Assumptions
- **NEVER assume a field is safe** - when in doubt, mask it

---

## Field Classification

### SAFE (always log)

| Category | Fields |
|----------|--------|
| System | `service`, `region`, `environment` |
| Correlation | `trace_id`, `span_id`, `request_id` |
| Operation | `method`, `status_code`, `error_type`, `operation` |
| Timing | `duration_ms`, `timestamp` |
| Enums | `grant_type`, `codec`, `media_type` |

### UNSAFE (require visibility selection)

| Category | Fields | Default |
|----------|--------|---------|
| Credentials | `password`, `secret`, `api_key`, `token`, `private_key` | Mask |
| PII | `email`, `phone`, `name`, `ip_address`, `user_agent` | Mask |
| Session | `meeting_id`, `participant_id` | Hash if correlation needed |
| Payloads | `request_body`, `response_body`, `error_message` | Mask |

### Visibility Levels

| Level | Output | Use Case |
|-------|--------|----------|
| Masked | `****` | Default for UNSAFE fields |
| Hashed | `h:a1b2c3d4` | When correlation needed |
| Plaintext | Full value | DEBUG only, dev only |

---

## Guards

**`scripts/guards/simple/no-secrets-in-logs.sh`** detects:
- `#[instrument]` without `skip(...)` on functions with secret parameters
- Log macros containing secret variable patterns
- Named tracing fields with secret names

**`scripts/guards/semantic/credential-leak.sh`** analyzes:
- Complex control flow where secrets might leak
- Struct definitions containing secrets that might be logged
- Error handling paths with secret data
