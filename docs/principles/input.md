# Principle: Input Validation

## Context

Input validation is the first line of defense against injection attacks, data corruption, and denial-of-service vulnerabilities. All external inputs must be validated at system boundaries before processing.

**Security Philosophy**: Fail fast, reject early, trust nothing from external sources.

## DO

### 1. Validate at System Boundaries
- **API Handlers**: Validate all HTTP request parameters, headers, and body content before processing
- **Message Receivers**: Validate all WebTransport/Protocol Buffer messages on arrival
- **Database Inputs**: Use parameterized queries (sqlx) exclusively - NEVER string concatenation
- **File Uploads**: Validate file types, sizes, and content before storage

### 2. Enforce Size Limits
- **JWT Tokens**: Maximum 4KB (`MAX_JWT_SIZE_BYTES`) to prevent DoS attacks
- **String Fields**: Define explicit length limits for usernames, client_ids, scopes
- **Request Bodies**: Limit JSON/Protobuf payload sizes at the HTTP layer
- **Arrays/Collections**: Limit number of elements to prevent memory exhaustion

### 3. Type Validation (Parse, Don't Validate)
- **Use Rust's type system**: Parse strings into typed wrappers (`ClientId`, `UserId`, `Scope`)
- **Enums for known values**: Use `ServiceType` enum instead of freeform strings
- **Reject invalid formats early**: Validate UUIDs, timestamps, base64 before use
- **Let serde fail fast**: Use `#[serde(deny_unknown_fields)]` where appropriate

### 4. Reject Invalid Input Immediately
- **No silent fallbacks**: Return errors instead of substituting default values
- **Fail before allocation**: Check size limits before base64 decode or parsing
- **Early error returns**: Use `?` operator to propagate validation errors up
- **Don't process partial data**: Validate entire request before starting work

### 5. Whitelist Allowed Characters
- **Client IDs**: `[a-zA-Z0-9_-]` only, no special characters or Unicode
- **Scopes**: Colon-separated namespace format (`meeting:create`, `media:process`)
- **Key IDs**: Alphanumeric with hyphens, no path traversal characters
- **Enum values**: Validate against known variants (`ServiceType::from_str`)

### 6. Validate Enum Values
```rust
// DO: Use FromStr with explicit error handling
impl FromStr for ServiceType {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "global-controller" => Ok(ServiceType::GlobalController),
            "meeting-controller" => Ok(ServiceType::MeetingController),
            "media-handler" => Ok(ServiceType::MediaHandler),
            _ => Err(format!("Invalid service type: {}", s)),
        }
    }
}
```

### 7. Check Numeric Ranges
- **Timestamps**: Validate `iat` is not too far in future (clock skew tolerance: 5 minutes)
- **Expiry**: Ensure `exp > iat` and within reasonable bounds (max 24 hours)
- **Port numbers**: Range 1-65535 for network configuration
- **Count limits**: Rate limits, pagination sizes, batch operation sizes

### 8. Context-Specific Sanitization
- **Logging**: Hash sensitive fields (`client_id`) before logging for correlation
- **SQL**: Always use parameterized queries (sqlx compile-time checks)
- **Shell**: NEVER pass user input to shell commands (use APIs instead)
- **HTML**: Not applicable (no server-side rendering in Dark Tower)

### 9. Use Typed Wrappers for Validated Data
```rust
// DO: Create newtype wrappers for validated inputs
#[derive(Debug, Clone)]
pub struct ValidatedClientId(String);

impl ValidatedClientId {
    pub fn new(s: String) -> Result<Self, ValidationError> {
        if s.len() > 64 {
            return Err(ValidationError::TooLong);
        }
        if !s.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_') {
            return Err(ValidationError::InvalidCharacters);
        }
        Ok(Self(s))
    }
}
```

### 10. Fuzz Test All Parsers
- **JWT validation**: Test with malformed tokens, oversized payloads, corrupted signatures
- **Protocol Buffers**: Fuzz signaling message parsers
- **Base64 decode**: Test with invalid padding, non-base64 characters
- **JSON parsing**: Test with deeply nested objects, invalid UTF-8

## DON'T

### 1. Don't Trust Client-Provided Data
```rust
// DON'T: Use client-provided values without validation
let user_id = request.headers.get("X-User-Id").unwrap(); // WRONG

// DO: Validate and parse into typed value
let user_id = request.headers
    .get("X-User-Id")
    .ok_or(AuthError::MissingHeader)?
    .to_str()
    .map_err(|_| AuthError::InvalidHeader)?
    .parse::<Uuid>()
    .map_err(|_| AuthError::InvalidUserId)?;
```

### 2. Don't Use String Concatenation for SQL
```rust
// DON'T: SQL injection vulnerability
let query = format!("SELECT * FROM users WHERE username = '{}'", username);

// DO: Use sqlx parameterized queries
let user = sqlx::query_as!(
    User,
    "SELECT * FROM users WHERE username = $1",
    username
)
.fetch_one(&pool)
.await?;
```

### 3. Don't Skip Validation for "Internal" Services
```rust
// DON'T: Assume service-to-service calls are safe
async fn handle_internal_request(data: String) {
    // Process without validation - WRONG
}

// DO: Validate all inputs, even from internal services (defense-in-depth)
async fn handle_internal_request(data: String) -> Result<(), Error> {
    let validated = ValidatedData::parse(data)?;
    // Process validated data
}
```

### 4. Don't Use `unwrap()` on User Input
```rust
// DON'T: Panic on invalid input
let token = headers.get("Authorization").unwrap(); // WRONG

// DO: Return error for invalid input
let token = headers.get("Authorization")
    .ok_or(AcError::MissingAuthHeader)?;
```

### 5. Don't Allow Unbounded Input Sizes
```rust
// DON'T: Accept arbitrary-sized payloads
let body: Vec<u8> = request.body().await?; // WRONG

// DO: Enforce size limits
if request_body.len() > MAX_BODY_SIZE {
    return Err(Error::PayloadTooLarge);
}
```

### 6. Don't Leak Information in Validation Errors
```rust
// DON'T: Reveal whether username exists
if !user_exists { return Err("User not found") }
if !password_valid { return Err("Invalid password") }

// DO: Use consistent error message
if !user_exists || !password_valid {
    return Err(AcError::InvalidCredentials); // Same error for both
}
```

### 7. Don't Normalize Input Silently
```rust
// DON'T: Auto-correct invalid input
let username = input.trim().to_lowercase(); // Silent normalization

// DO: Reject invalid format, require correct input
if input != input.trim() || input.chars().any(|c| c.is_uppercase()) {
    return Err(ValidationError::InvalidFormat);
}
```

## Examples

### Good: JWT Size Limit Enforcement
```rust
// From: crates/ac-service/src/crypto/mod.rs
pub fn verify_jwt(token: &str, public_key: &str) -> Result<Claims, AcError> {
    // Check size BEFORE any parsing or crypto operations
    if token.len() > MAX_JWT_SIZE_BYTES {
        return Err(AcError::InvalidToken(
            "JWT exceeds maximum allowed size".to_string()
        ));
    }

    // Proceed with signature verification
    let decoding_key = DecodingKey::from_ed_pem(public_key.as_bytes())?;
    let validation = Validation::new(Algorithm::EdDSA);
    let token_data = decode::<Claims>(token, &decoding_key, &validation)?;

    Ok(token_data.claims)
}
```

### Good: Enum Validation with Explicit Errors
```rust
// From: crates/ac-service/src/models/mod.rs
impl FromStr for ServiceType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "global-controller" => Ok(ServiceType::GlobalController),
            "meeting-controller" => Ok(ServiceType::MeetingController),
            "media-handler" => Ok(ServiceType::MediaHandler),
            _ => Err(format!("Invalid service type: {}", s)),
        }
    }
}
```

### Good: Scope Validation
```rust
// From: crates/ac-service/src/services/token_service.rs
// Verify requested scopes are subset of allowed scopes
let scopes = if let Some(req_scopes) = requested_scopes {
    let all_valid = req_scopes.iter().all(|s| credential.scopes.contains(s));

    if !all_valid {
        return Err(AcError::InsufficientScope {
            required: req_scopes.join(" "),
            provided: credential.scopes.clone(),
        });
    }
    req_scopes
} else {
    credential.scopes.clone()
};
```

### Bad: Trusting User Input
```rust
// DON'T DO THIS
async fn delete_user(user_id: String) -> Result<(), Error> {
    // Directly using user_id without validation
    let query = format!("DELETE FROM users WHERE id = '{}'", user_id);
    sqlx::query(&query).execute(&pool).await?;
    Ok(())
}
```

### Good: Parameterized Queries
```rust
// DO THIS INSTEAD
async fn delete_user(user_id: Uuid) -> Result<(), Error> {
    // Type system ensures user_id is valid UUID
    // sqlx ensures query is parameterized
    sqlx::query!("DELETE FROM users WHERE id = $1", user_id)
        .execute(&pool)
        .await?;
    Ok(())
}
```

## Guards

Input validation is enforced through multiple mechanisms:

1. **Compile-Time Guards**:
   - `sqlx::query!` macro - Compile-time SQL validation, parameterization enforced
   - Type system - Strong typing prevents category errors
   - `#[serde(deny_unknown_fields)]` - Reject unexpected JSON fields

2. **Runtime Guards**:
   - Size limits checked before allocation (`MAX_JWT_SIZE_BYTES`)
   - Enum parsing with explicit error handling (`FromStr`)
   - Timestamp validation (clock skew tolerance)
   - Rate limiting on authentication endpoints

3. **Test Coverage Guards**:
   - P0 security tests for injection vulnerabilities (SQL, JWT payload tampering)
   - P1 fuzz tests for all parsers (JWT, Protocol Buffers, base64)
   - Integration tests for validation edge cases

4. **Code Review Guards**:
   - NEVER allow `unwrap()` on external input
   - ALWAYS use sqlx parameterized queries
   - REQUIRE size limits on all variable-length inputs
   - ENFORCE enum validation for all categorical data

## ADR References

- **ADR-0002**: No-Panic Policy - Validates must return `Result`, never panic on bad input
- **ADR-0003**: Service Authentication - JWT validation, clock skew tolerance, scope checking
- **ADR-0005**: Integration Testing - Requires validation tests for all system boundaries
- **ADR-0006**: Fuzz Testing Strategy - All parsers must have fuzz tests
- **ADR-0007**: Token Lifetime Strategy - Timestamp validation rules (`iat`, `exp`)

## Related Principles

- **Error Handling**: Validation errors must use typed error enums (`AcError::InvalidToken`)
- **Cryptography**: Validate key formats and sizes before cryptographic operations
- **Database**: All queries must be parameterized (compile-time verified by sqlx)
- **Observability**: Hash sensitive inputs before logging for correlation
