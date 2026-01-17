# Test Specialist - Patterns

Testing patterns worth documenting for Dark Tower codebase.

---

## Pattern: Config Boundary Testing
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/config.rs`

Test all valid values (min, default, max) plus invalid values on both sides of boundaries. The bcrypt cost tests demonstrate this well: test 10 (min), 11, 12 (default), 13, 14 (max), then test 9 (below min) and 15 (above max). Always include the exact boundary values.

---

## Pattern: Defense-in-Depth Validation Tests
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/crypto/mod.rs`

When a function validates input that config already validated, test that the function still rejects invalid inputs. In `hash_client_secret()`, cost validation exists both in config AND the function. Test both layers independently. This catches bugs if callers bypass config.

---

## Pattern: Cross-Version Verification Tests
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/crypto/mod.rs`

For migration scenarios (bcrypt cost changes, algorithm upgrades), test that old artifacts verify correctly with new code. The `test_hash_verification_works_across_cost_factors` test creates hashes at costs 10-14 and verifies ALL of them work regardless of current config. Essential for zero-downtime deployments.

---

## Pattern: Constant Assertion Tests
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/config.rs`, `crates/ac-service/src/crypto/mod.rs`

Document security-critical constants with dedicated assertion tests. Tests like `test_bcrypt_cost_constants_are_valid()` verify DEFAULT >= MIN and DEFAULT <= MAX. Self-documenting and catch copy-paste errors in constant definitions.

---

## Pattern: Handler Integration with Config Propagation
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/handlers/admin_handler.rs`

When config values flow through handlers to service functions, test the full chain. The `handle_register_service` and `handle_rotate_client_secret` handlers pass `state.config.bcrypt_cost` to crypto functions. Integration tests verify config actually reaches the crypto layer.

---

## Pattern: Hash Format Verification
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/crypto/mod.rs`

For bcrypt/argon2/etc, verify hash structure matches expected format. Parse the hash string (e.g., `$2b$12$...`) and assert version and cost separately. This catches silent algorithm downgrades.

---

## Pattern: Error Message Content Tests
**Added**: 2026-01-11
**Related files**: `crates/ac-service/src/config.rs`

When testing invalid inputs, verify error messages contain useful context. Tests like `test_bcrypt_cost_rejects_too_low` check that the error message mentions the valid range (10-14). Helps users self-diagnose config issues.

---

## Pattern: SecretBox Debug Redaction Tests
**Added**: 2026-01-12
**Related files**: `crates/ac-service/src/crypto/mod.rs`, `crates/ac-service/src/models/mod.rs`, `crates/ac-service/src/config.rs`

When struct contains `SecretBox<T>` or `SecretString`, test the Debug impl:
```rust
#[test]
fn test_struct_debug_redacts_secret() {
    let s = MyStruct { secret: SecretString::from("hunter2"), public: "visible" };
    let debug = format!("{:?}", s);
    assert!(debug.contains("[REDACTED]"), "Secret should be redacted");
    assert!(!debug.contains("hunter2"), "Actual value must not appear");
    assert!(debug.contains("visible"), "Public fields should appear");
}
```
This prevents accidental credential leaks in logs.

---

## Pattern: SecretBox Value Access Tests
**Added**: 2026-01-12
**Related files**: `crates/ac-service/src/crypto/mod.rs`

When testing functions that return `SecretString` or use `SecretBox`, always access via `.expose_secret()`:
```rust
#[test]
fn test_generate_secret() {
    let secret = generate_client_secret().unwrap();
    // WRONG: secret.as_str() - won't compile
    // RIGHT: explicit exposure
    assert!(!secret.expose_secret().is_empty());
}
```
The compiler enforces this, making accidental exposure impossible.

---

## Pattern: Custom Clone Tests for SecretBox Structs
**Added**: 2026-01-12
**Related files**: `crates/ac-service/src/crypto/mod.rs`

Structs with `SecretBox` fields require manual Clone impl. Test that Clone produces correct deep copies:
```rust
#[test]
fn test_encrypted_key_clone() {
    let original = EncryptedKey {
        encrypted_data: SecretBox::new(Box::new(vec![1,2,3])),
        nonce: vec![4,5,6],
        tag: vec![7,8,9],
    };
    let cloned = original.clone();
    assert_eq!(cloned.encrypted_data.expose_secret(), original.encrypted_data.expose_secret());
    assert_eq!(cloned.nonce, original.nonce);
}
```

---

## Pattern: Wrapper Type Refactor Verification
**Added**: 2026-01-12
**Related files**: Integration test files

When refactoring raw types to wrapper types (e.g., `Vec<u8>` to `SecretBox<Vec<u8>>`):
1. Search all usages of the struct being modified
2. Update construction sites to wrap values: `SecretBox::new(Box::new(value))`
3. Update access sites to unwrap: `.expose_secret()`
4. **Verify test files are included in mod.rs** - orphaned tests won't catch type errors
5. Run `cargo test` and verify expected test count executes

---

## Pattern: NetworkPolicy Positive/Negative Test Pair
**Added**: 2026-01-13
**Related files**: `crates/env-tests/tests/40_resilience.rs`, `crates/env-tests/src/canary.rs`

When testing NetworkPolicy enforcement, always implement paired tests:
1. **Positive test** (same namespace): Deploy canary with allowed labels, verify connectivity WORKS
2. **Negative test** (cross namespace): Deploy canary in different namespace, verify connectivity BLOCKED

Interpretation matrix:
- Positive passes, negative fails = NetworkPolicy working correctly
- Both pass = NetworkPolicy NOT enforced (security gap!)
- Positive fails = Service down OR NetworkPolicy misconfigured (blocking all traffic)

Always run positive test first to validate test infrastructure works.

---

## Pattern: Cluster-Dependent Test Structure
**Added**: 2026-01-13
**Related files**: `crates/env-tests/tests/*.rs`

For tests requiring a running cluster, follow this structure:
```rust
#![cfg(feature = "flows")]  // Feature-gate to prevent accidental runs

async fn cluster() -> ClusterConnection {
    ClusterConnection::new()
        .await
        .expect("Failed to connect - ensure port-forwards are running")
}

#[tokio::test]
async fn test_feature() {
    let cluster = cluster().await;
    let client = ServiceClient::new(&cluster.service_base_url);
    // ... test logic
}
```
Use feature gates (smoke, flows, observability, resilience) to categorize test execution time.

---

## Pattern: CanaryPod for In-Cluster Testing
**Added**: 2026-01-13
**Related files**: `crates/env-tests/src/canary.rs`

For testing cluster-internal behavior (NetworkPolicies, service mesh, etc.), use CanaryPod pattern:
```rust
let canary = CanaryPod::deploy("target-namespace").await?;
let can_reach = canary.can_reach("http://service:port/health").await;
canary.cleanup().await?;  // Also cleaned on Drop
```
Key design decisions:
- Use `std::process::Command` to call kubectl (not async kubectl client)
- Implement `Drop` for automatic cleanup even on test panic
- Use `AtomicBool` to prevent double-cleanup
- Generate unique pod names with UUIDs to avoid collisions

---

## Pattern: JWT Header Injection Test Suite
**Added**: 2026-01-13
**Related files**: `crates/env-tests/tests/25_auth_security.rs`

Test all JWT header injection attack vectors:
1. **kid injection**: Path traversal (`../../etc/passwd`), SQL injection (`'; DROP TABLE`), XSS, null bytes
2. **jwk injection**: Embedded attacker key (CVE-2018-0114)
3. **jku injection**: External URL, internal SSRF, file:// protocol

For each vector, craft a tampered header and verify signature validation fails:
```rust
let new_header = serde_json::json!({ "alg": "EdDSA", "typ": "JWT", "kid": malicious_value });
let tampered = format!("{}.{}.{}", encode(new_header), original_payload, original_sig);
assert!(decode(&tampered, &key, &validation).is_err());
```
Signature mismatch proves the attack was rejected (header is part of signed data).

---

## Pattern: JWKS Private Key Leakage Validation
**Added**: 2026-01-13
**Related files**: `crates/env-tests/tests/25_auth_security.rs`

Validate JWKS endpoint does not expose private key fields (CWE-321):
```rust
let private_key_fields = ["d", "p", "q", "dp", "dq", "qi"];
for key in jwks_value["keys"].as_array().unwrap() {
    for field in &private_key_fields {
        assert!(key.get(*field).is_none(),
            "JWKS exposes private key field '{}'", field);
    }
}
```
Fetch raw JSON to check all fields - typed deserialization may skip unknown fields.

---

## Pattern: Test Server Harness for Integration HTTP Testing
**Added**: 2026-01-14
**Related files**: `crates/gc-test-utils/src/server_harness.rs`, `crates/global-controller/tests/health_tests.rs`

For HTTP service integration testing, create a reusable server harness that:
1. Spawns a real HTTP server on a random available port (127.0.0.1:0)
2. Provides access to the database pool for assertions
3. Implements Drop for automatic cleanup
4. Uses `#[sqlx::test(migrations = "...")]` for database setup

```rust
pub struct TestGcServer {
    addr: SocketAddr,
    pool: PgPool,
    _handle: JoinHandle<()>,
}

impl TestGcServer {
    pub async fn spawn(pool: PgPool) -> Result<Self, anyhow::Error> {
        // 1. Create config from test vars
        let config = Config::from_vars(&test_vars)?;

        // 2. Build app state and routes
        let app = routes::build_routes(Arc::new(AppState { pool, config }));

        // 3. Bind to random port
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let addr = listener.local_addr()?;

        // 4. Spawn server in background
        let handle = tokio::spawn(async move {
            axum::serve(listener, app.into_make_service()).await.ok()
        });

        Ok(Self { addr, pool, _handle: handle })
    }

    pub fn url(&self) -> String {
        format!("http://{}", self.addr)
    }
}
```

Usage in tests:
```rust
#[sqlx::test(migrations = "../../migrations")]
async fn test_endpoint(pool: PgPool) -> Result<(), anyhow::Error> {
    let server = TestGcServer::spawn(pool).await?;
    let client = reqwest::Client::new();
    let response = client.get(&format!("{}/v1/health", server.url())).send().await?;
    assert_eq!(response.status(), 200);
    Ok(())
}
```

Key benefits:
- Real HTTP server, not mocked
- Runs database migrations automatically (via sqlx::test)
- Random ports prevent conflicts in parallel test execution
- Drop impl ensures server stops on test completion
- Direct database pool access for assertions

---

## Pattern: Manual Debug Redaction Alternative
**Added**: 2026-01-14
**Related files**: `crates/global-controller/src/config.rs`

For sensitive configuration that doesn't use SecretBox/SecretString, implement manual Debug:
```rust
impl fmt::Debug for Config {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Config")
            .field("public_field", &self.public_field)
            .field("secret_field", &"[REDACTED]")  // Redact sensitive
            .finish()
    }
}
```

Test this pattern:
```rust
#[test]
fn test_debug_redacts_secrets() {
    let config = Config { ..., secret_field: "actual_secret" };
    let debug = format!("{:?}", config);

    assert!(debug.contains("[REDACTED]"));
    assert!(!debug.contains("actual_secret"));
}
```

This is appropriate for simple config values that don't need zeroizing. SecretBox/SecretString is preferred for cryptographic material.

---

## Pattern: Serde skip_serializing_if for Optional Response Fields
**Added**: 2026-01-14
**Related files**: `crates/global-controller/src/models/mod.rs`

For API responses with optional fields, use `#[serde(skip_serializing_if = "Option::is_none")]` to omit None fields from JSON. Test both cases:

```rust
#[derive(Serialize)]
pub struct HealthResponse {
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub database: Option<String>,
}
```

Tests:
```rust
#[test]
fn test_serialization_without_optional() {
    let response = HealthResponse {
        status: "ok".into(),
        database: None,
    };
    let json = serde_json::to_string(&response).unwrap();
    assert!(!json.contains("database"));  // Field omitted!
}

#[test]
fn test_serialization_with_optional() {
    let response = HealthResponse {
        status: "ok".into(),
        database: Some("healthy".into()),
    };
    let json = serde_json::to_string(&response).unwrap();
    assert!(json.contains("database"));
}

#[test]
fn test_deserialization_missing_optional() {
    let json = r#"{"status":"ok"}"#;
    let response: HealthResponse = serde_json::from_str(json).unwrap();
    assert_eq!(response.database, None);
}
```

Essential for REST APIs to maintain clean JSON and avoid breaking clients when adding optional fields.

---

## Pattern: Boundary Testing for Security Limits
**Added**: 2026-01-14
**Related files**: `crates/global-controller/src/auth/jwt.rs`, `crates/global-controller/tests/auth_tests.rs`

For security-critical size limits (token size, buffer sizes, etc.), test the boundary explicitly:
1. **At limit**: Size exactly at boundary should be accepted
2. **Below limit**: Size below boundary should be accepted
3. **Above limit**: Size above boundary should be rejected
4. **Far above limit**: Much larger size should be rejected (catches off-by-one in both directions)

Example for JWT 8KB limit:
```rust
#[test]
fn test_token_exactly_at_8kb_limit_accepted() {
    // Create valid token padded to exactly 8192 bytes
    let token = create_valid_token_with_size(8192);
    assert_eq!(validate_token(&token).is_ok(), true);
}

#[test]
fn test_token_at_8193_bytes_rejected() {
    // Create valid token at 8193 bytes (1 byte over limit)
    let token = create_valid_token_with_size(8193);
    assert!(validate_token(&token).is_err());
}
```

Why this matters: Off-by-one errors in size checks can allow DoS attacks. Test the exact boundary, not just "large" vs "small".

---

## Pattern: Algorithm Confusion Attack Testing
**Added**: 2026-01-14
**Related files**: `crates/global-controller/src/auth/jwt.rs`, `crates/global-controller/tests/auth_tests.rs`

For JWT validation, test algorithm confusion attacks where attacker changes the algorithm in the token header:
1. **Test `alg:none`**: Token with no algorithm should be rejected (CVE-2016-10555)
2. **Test alternative algorithms**: Token with HS256/RS256 when only EdDSA is supported should be rejected (CVE-2017-11424)
3. **Test correct algorithm**: Valid EdDSA token should pass

```rust
#[test]
fn test_token_with_alg_none_rejected() {
    let token = create_token_with_header_override(json!({"alg": "none", "typ": "JWT"}), valid_claims);
    let result = validate_token(&token);
    assert!(result.is_err(), "alg:none must be rejected");
    assert_eq!(result.unwrap_err().status_code(), StatusCode::UNAUTHORIZED);
}

#[test]
fn test_token_with_alg_hs256_rejected() {
    let token = create_token_with_header_override(json!({"alg": "HS256", "typ": "JWT"}), valid_claims);
    let result = validate_token(&token);
    assert!(result.is_err(), "HS256 must be rejected when EdDSA is expected");
}

#[test]
fn test_only_eddsa_algorithm_accepted() {
    let token = create_valid_eddsa_token();
    assert!(validate_token(&token).is_ok());
}
```

Why this matters: Algorithm confusion is a common JWT vulnerability. Always explicitly test that ONLY the expected algorithm is accepted, not just "any valid JWT".

---

## Pattern: JWK Structure Validation
**Added**: 2026-01-14
**Related files**: `crates/global-controller/src/auth/jwt.rs`

When validating JWKs from a JWKS endpoint, validate the key structure itself, not just use it:
1. **Verify kty (key type)**: EdDSA uses `"OKP"`, not `"RSA"` or `"EC"`
2. **Verify alg (algorithm)**: When present in JWK, must match expected algorithm
3. **Log warnings**: If validation fails, log the mismatch before rejecting (helps operators debug misconfigurations)

```rust
pub fn verify_token(token: &str, jwk: &Jwk) -> Result<Claims, GcError> {
    // Validate JWK structure before attempting verification
    if jwk.kty != "OKP" {
        tracing::warn!(
            expected = "OKP",
            actual = jwk.kty,
            "JWK has incorrect key type (expected OKP for EdDSA)"
        );
        return Err(GcError::Unauthorized("invalid or expired token".to_string()));
    }

    if let Some(alg) = &jwk.alg {
        if alg != "EdDSA" {
            tracing::warn!(
                expected = "EdDSA",
                actual = alg,
                "JWK has incorrect algorithm"
            );
            return Err(GcError::Unauthorized("invalid or expired token".to_string()));
        }
    }

    // Now safe to use the key
    // ...
}
```

Why this matters: A compromised JWKS endpoint might return keys with wrong algorithms, enabling signature confusion attacks. Validate the structure, not just trust the endpoint.

---

## Pattern: Layered JWT Testing (Defense-in-Depth)
**Added**: 2026-01-15
**Related files**: `crates/global-controller/tests/auth_tests.rs`

JWT security requires testing at multiple layers, not just the happy path:
1. **Token algorithm layer**: Reject `alg:none`, `alg:HS256`, accept only `alg:EdDSA`
2. **JWK structure layer**: Reject `kty != "OKP"`, reject `alg != "EdDSA"` (when present)
3. **Signature verification layer**: Reject tampered payloads
4. **Claims validation layer**: Reject expired tokens, invalid iat, missing required fields

Each layer is independent - a compromised JWKS endpoint or network MITM could bypass token-level checks, which is why JWK structure validation is essential. Test each layer separately:

```rust
#[test]
fn test_algorithm_confusion_attack_alg_none_rejected() {
    // Token layer: attack via header algorithm field
    let token = create_token_with_header_override(json!({"alg": "none", "typ": "JWT"}), valid_claims);
    assert!(validate_token(&token).is_err());
}

#[test]
fn test_jwk_structure_validation_rejects_wrong_kty() {
    // JWK layer: attack via key type mismatch
    let jwk = create_jwk_with_kty("RSA");  // Wrong type for EdDSA
    assert_eq!(verify_token_with_jwk(valid_token, &jwk).status(), 401);
}

#[test]
fn test_signature_validation_detects_tampering() {
    // Signature layer: payload modified after signing
    let tampered_payload = modify_jwt_payload(valid_token, |p| p["sub"] = "attacker");
    assert!(validate_token(&tampered_payload).is_err());
}
```

Why this matters: Algorithm confusion (CVE-2016-10555, CVE-2017-11424) is a real attack. Testing only "EdDSA works" misses the attacks that use `none` or `HS256`. Test must include both positive (EdDSA accepted) and negative cases (other algorithms rejected).

---

## Pattern: User Provisioning Test Coverage
**Added**: 2026-01-15
**Related files**: `crates/ac-service/tests/auth_tests.rs`, `crates/ac-service/tests/integration/user_service_tests.rs`

When testing user provisioning (registration, token issuance, claims), cover these test categories:

1. **Happy path**: Valid registration → valid token issuance → valid claims extraction
2. **Validation**: Username length, email format, password strength validation
3. **Rate limiting**: Prevent brute force registration, token issuance throttling
4. **Timing attack prevention**: Registration duration constant regardless of validation failure
5. **Claims structure**: All required fields present, optional fields handled, scopes serialized correctly

Example structure:
```rust
// Happy path
#[test]
fn test_user_registration_and_token_issuance() { ... }

// Validation boundaries
#[test]
fn test_register_user_rejects_short_username() { ... }
#[test]
fn test_register_user_rejects_invalid_email() { ... }

// Rate limiting
#[test]
fn test_register_user_rate_limiting() { ... }
#[test]
fn test_issue_user_token_rate_limiting() { ... }

// Timing
#[test]
fn test_registration_timing_constant_regardless_of_validation_failure() { ... }

// Claims
#[test]
fn test_user_claims_contains_all_required_fields() { ... }
#[test]
fn test_user_claims_scopes_serialized_correctly() { ... }
```

Integration tests should verify full flows:
- User registers → Auth service persists to database → User later logs in with credentials → Token issued with correct claims
- Test database interaction, not just function logic
- Use `#[sqlx::test(migrations = "../../migrations")]` for database isolation

---

## Pattern: Integration User Auth Test Organization
**Added**: 2026-01-15
**Related files**: `crates/ac-service/tests/integration/user_auth_tests.rs`

Organize user auth integration tests into logical groups with section comments:
1. **Registration tests** (11): happy path, token claims, default role, validation errors (email, password, display_name), duplicate email, multi-tenant (same email different orgs), invalid subdomain, unknown org, rate limiting
2. **Login tests** (7): happy path, token claims, last_login update, wrong password, nonexistent user, inactive user, rate limit lockout
3. **Org extraction tests** (4): valid subdomain, with port, IP rejected, uppercase rejected

Use `// =========` visual separators for navigating large test files. Each test follows Arrange-Act-Assert with explicit comments.

---

## Pattern: Rate Limiting Tests via Loop
**Added**: 2026-01-15
**Related files**: `crates/ac-service/tests/integration/user_auth_tests.rs`

Test rate limiting by sending requests in a loop until lockout triggers:
```rust
for i in 0..6 {
    let response = client.post(...).send().await?;
    if i < 5 {
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    } else {
        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    }
}
```
This approach validates that rate limiting kicks in after threshold without hardcoding timing assumptions. For registration, use unique emails per attempt; for login, use same invalid password.
