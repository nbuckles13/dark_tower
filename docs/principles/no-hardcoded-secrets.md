# Principle: No Hardcoded Secrets

**Status**: Active
**Guard**: `scripts/guards/simple/no-hardcoded-secrets.sh`

## Definition

Secrets (passwords, API keys, tokens, encryption keys, connection credentials) MUST NOT be hardcoded in source code. All secrets must be provided at runtime through environment variables, configuration files, or secret management systems.

## Why This Matters

1. **Version Control Exposure**: Hardcoded secrets are committed to git history permanently
2. **Access Control**: Anyone with repo access can extract secrets
3. **Rotation Difficulty**: Changing secrets requires code changes and redeployment
4. **Environment Separation**: Dev/staging/prod should use different credentials

## What Counts as a Secret

- Passwords and passphrases
- API keys (Stripe, AWS, GitHub, etc.)
- OAuth client secrets
- Database credentials
- Encryption keys (master keys, private keys)
- Bearer tokens
- Connection strings with embedded credentials
- Service account credentials

## Violation Patterns

### Pattern 1: Direct Assignment
```rust
// VIOLATION
let password = "super-secret-123";
let api_key = "sk-abc123def456...";
```

### Pattern 2: API Key Prefixes
```rust
// VIOLATION - recognizable API key formats
let stripe_key = "sk-live-abc123...";    // Stripe
let aws_key = "AKIAIOSFODNN7EXAMPLE";    // AWS
let github_token = "ghp_xxxx...";         // GitHub
```

### Pattern 3: Connection Strings
```rust
// VIOLATION
let db_url = "postgresql://admin:password123@prod-db.example.com:5432/mydb";
```

### Pattern 4: Authorization Headers
```rust
// VIOLATION
let auth = "Authorization: Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9...";
```

## Acceptable Patterns

### Environment Variables
```rust
// OK - secret loaded from environment
let password = std::env::var("DB_PASSWORD")?;
let api_key = dotenvy::var("API_KEY")?;
```

### Configuration Structs with Runtime Loading
```rust
// OK - populated from config file/env at runtime
#[derive(Deserialize)]
struct Config {
    #[serde(deserialize_with = "secret_string")]
    database_password: SecretString,
}
```

### SecretString Type
```rust
// OK - using type system for protection
use common::secret::SecretString;

fn authenticate(password: SecretString) {
    // Password is wrapped, cannot be accidentally logged
}
```

### Test Fixtures (in test code)
```rust
#[cfg(test)]
mod tests {
    // OK - test code is excluded from guard
    const TEST_PASSWORD: &str = "test-password";
}
```

### Placeholder Values
```rust
// OK - obviously not real secrets
let api_key = "your-api-key-here";  // Documentation example
let password = "changeme";           // Default placeholder
```

## Guard Implementation

The simple guard (`no-hardcoded-secrets.sh`) checks for:

| Check | Pattern | Example |
|-------|---------|---------|
| 1 | Secret variable assignments | `password = "actual-secret"` |
| 2 | API key prefixes | `"sk-..."`, `"AKIA..."` |
| 3 | Connection strings | `"postgresql://user:pass@host"` |
| 4 | Auth headers | `"Authorization: Bearer xyz..."` |
| 5 | Long base64 (review) | 40+ char base64 strings |

### Exclusions

The guard automatically excludes:
- Test files and `#[cfg(test)]` blocks (via compiler-based detection)
- Environment variable references (`std::env`, `env::var`, `dotenvy`)

## Resolution Strategies

1. **Environment Variables**: Best for deployment-specific values
   ```rust
   let secret = std::env::var("MY_SECRET")?;
   ```

2. **Configuration Files**: For structured config (not committed)
   ```rust
   let config: Config = config::Config::builder()
       .add_source(config::File::with_name("config"))
       .build()?;
   ```

3. **Secret Management**: For production (Vault, AWS Secrets Manager)
   ```rust
   let secret = vault_client.get_secret("path/to/secret").await?;
   ```

4. **SecretString**: Wrap runtime secrets to prevent logging
   ```rust
   let password = SecretString::from(env_password);
   ```

## Related Principles

- **[logging-safety.md](logging-safety.md)**: Don't log secrets at runtime
- Both work together: no-hardcoded-secrets prevents secrets in code, logging-safety prevents secrets in logs
