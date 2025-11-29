# "Fuzz This" Checklist

This checklist helps developers and code reviewers identify when fuzz testing is required. Use this during design, implementation, and code review.

## Quick Decision Tree

```
Is the code parsing untrusted input?
├─ YES → Network data?
│  └─ YES → ✅ CRITICAL - Fuzz required
│  └─ NO → User input?
│     └─ YES → ✅ HIGH - Fuzz required
│     └─ NO → Database/config?
│        └─ ✅ MEDIUM - Fuzz recommended
└─ NO → Does it perform cryptographic operations?
   └─ YES → ✅ CRITICAL - Fuzz required
   └─ NO → Complex data transformation?
      └─ YES → ✅ LOW - Fuzz optional
      └─ NO → ❌ Fuzz not needed
```

## Pattern Recognition: Fuzz-Worthy Code

### 🔴 CRITICAL - Fuzz Required (Block Merge if Missing)

#### Pattern 1: Network Data Parser

```rust
// ❌ NO FUZZ TARGET
pub fn parse_media_frame(data: &[u8]) -> Result<MediaFrame> {
    // Parses 42-byte header from untrusted WebTransport datagram
}

// ✅ REQUIRED: fuzz/fuzz_targets/parse_media_frame.rs
```

**Why**: Network data is completely untrusted, any parser bug can cause DoS or RCE.

**Examples in Dark Tower**:
- `media_protocol::codec::decode_frame()` - Media frame codec
- `proto_gen::signaling::ClientMessage::decode()` - Protocol Buffer parser
- WebTransport datagram handlers

#### Pattern 2: Cryptographic Operations

```rust
// ❌ NO FUZZ TARGET
pub fn validate_jwt(token: &str, key: &PublicKey) -> Result<Claims> {
    // JWT parsing and signature validation
}

// ✅ REQUIRED: fuzz/fuzz_targets/jwt_validation.rs
```

**Why**: Auth bypass if parser has bugs. Signature validation must be robust.

**Examples in Dark Tower**:
- `ac_service::crypto::jwt::validate_token()` - JWT validation
- `ac_service::crypto::encryption::decrypt_private_key()` - AES-GCM decryption
- Any EdDSA signature verification

#### Pattern 3: Binary Protocol Codec

```rust
// ❌ NO FUZZ TARGET
pub fn encode_frame(frame: &MediaFrame) -> Result<Bytes> {
    // Encodes to 42-byte header + payload
}

pub fn decode_frame(data: &[u8]) -> Result<MediaFrame> {
    // Decodes from bytes
}

// ✅ REQUIRED:
//   - fuzz/fuzz_targets/codec_decode.rs (malformed input)
//   - fuzz/fuzz_targets/codec_roundtrip.rs (encode→decode correctness)
```

**Why**: Binary codecs have complex parsing logic prone to buffer overflows, integer overflows.

**Examples in Dark Tower**:
- Media frame codec (42-byte header)
- Any custom binary serialization
- RTP/RTCP packet parsers (future)

---

### 🟠 HIGH - Fuzz Required (Fix Before Merge)

#### Pattern 4: HTTP Request Handler

```rust
// ❌ NO FUZZ TARGET
pub async fn handle_token_request(
    Json(req): Json<TokenRequest>
) -> Result<Json<TokenResponse>> {
    // Parses JSON request body
}

// ✅ REQUIRED: fuzz/fuzz_targets/http_handlers.rs
```

**Why**: HTTP endpoints accept untrusted JSON, query params, headers.

**Examples in Dark Tower**:
- All `/api/v1/*` endpoints
- JSON request body parsing
- Query parameter parsing

#### Pattern 5: Database Input Validation

```rust
// ❌ NO FUZZ TARGET
pub async fn create_user(
    db: &PgPool,
    username: &str,
    email: &str
) -> Result<User> {
    sqlx::query("INSERT INTO users (username, email) VALUES ($1, $2)")
        .bind(username)
        .bind(email)
        .execute(db)
        .await
}

// ✅ REQUIRED: fuzz/fuzz_targets/db_inputs.rs
```

**Why**: SQL injection, constraint violations, special character handling.

**Examples in Dark Tower**:
- All `repositories/*.rs` functions
- Any `sqlx::query!` with user input
- Database constraint validation

#### Pattern 6: Complex Deserialization

```rust
// ❌ NO FUZZ TARGET
#[derive(serde::Deserialize)]
pub struct MeetingConfig {
    max_participants: u32,
    settings: HashMap<String, serde_json::Value>,
}

pub fn parse_config(json: &str) -> Result<MeetingConfig> {
    serde_json::from_str(json)
}

// ✅ REQUIRED: fuzz/fuzz_targets/meeting_config.rs
```

**Why**: Nested JSON, arbitrary structures, type coercion bugs.

**Examples in Dark Tower**:
- Meeting configuration parsing
- User settings deserialization
- Any `serde_json::from_str()` with complex types

---

### 🟡 MEDIUM - Fuzz Recommended (Fix Soon)

#### Pattern 7: Internal Protocol Messages

```rust
// ⚠️ NO FUZZ TARGET (but should have one)
pub fn serialize_internal_command(cmd: &InternalCommand) -> Vec<u8> {
    // Serialize for service-to-service communication
}

// ✅ RECOMMENDED: fuzz/fuzz_targets/internal_protocol.rs
```

**Why**: Even trusted services can send malformed data (bugs, compromised service).

**Examples in Dark Tower**:
- `proto/internal.proto` messages
- Meeting Controller ↔ Media Handler communication
- Global Controller ↔ Meeting Controller messages

#### Pattern 8: Configuration File Parsing

```rust
// ⚠️ NO FUZZ TARGET
pub fn load_config(path: &Path) -> Result<Config> {
    let contents = fs::read_to_string(path)?;
    toml::from_str(&contents)
}

// ✅ RECOMMENDED: fuzz/fuzz_targets/config_parser.rs
```

**Why**: Config files can be modified by attackers with file access.

**Examples in Dark Tower**:
- TOML configuration parsing
- Environment variable parsing
- Feature flags

---

### 🟢 LOW - Fuzz Optional (Nice to Have)

#### Pattern 9: Complex Data Transformations

```rust
// Optional fuzz target
pub fn optimize_layout(participants: &[Participant], grid: GridConfig) -> Layout {
    // Complex layout algorithm
}

// ✅ OPTIONAL: fuzz/fuzz_targets/layout_optimizer.rs
```

**Why**: Can find edge cases in business logic, but not security-critical.

**Examples in Dark Tower**:
- Layout computation algorithms
- Bandwidth allocation logic
- Routing optimization

#### Pattern 10: State Machine Transitions

```rust
// Optional fuzz target
impl MeetingState {
    pub fn transition(&mut self, event: Event) -> Result<()> {
        // Complex state transitions
    }
}

// ✅ OPTIONAL: fuzz/fuzz_targets/meeting_state_machine.rs
```

**Why**: Can find invalid state transitions, but usually caught by unit tests.

---

### ❌ Fuzz Not Needed

#### Pattern: Simple Getters/Setters

```rust
// No fuzz needed
pub fn get_participant_count(&self) -> usize {
    self.participants.len()
}

pub fn set_display_name(&mut self, name: String) {
    self.display_name = name;
}
```

#### Pattern: Pure Computation (Trusted Input)

```rust
// No fuzz needed
fn calculate_checksum(data: &[u8]) -> u32 {
    data.iter().fold(0, |acc, &b| acc.wrapping_add(b as u32))
}
```

#### Pattern: Logging/Metrics

```rust
// No fuzz needed
fn log_event(event: &str) {
    tracing::info!("Event: {}", event);
}
```

---

## Code Review Checklist

When reviewing a PR, check these items:

### For New Parsers

- [ ] **Input source identified**: Network / User / Database / Config / Trusted
- [ ] **Fuzz priority assigned**: CRITICAL / HIGH / MEDIUM / LOW
- [ ] **Fuzz target exists** (if priority ≥ MEDIUM):
  - [ ] Located in `crates/*/fuzz/fuzz_targets/`
  - [ ] Compiles: `cargo fuzz build target_name`
  - [ ] Runs without crashes: `cargo fuzz run target_name -- -runs=10000`
- [ ] **Seed corpus created** (if priority = CRITICAL):
  - [ ] At least 10 valid inputs in `fuzz/corpus/target_name/`
- [ ] **Added to CI** (if priority ≥ HIGH):
  - [ ] Listed in `.github/workflows/fuzz.yml`
- [ ] **Documentation updated**:
  - [ ] Comment in code explaining fuzz coverage
  - [ ] Updated `docs/FUZZING.md` if new technique used

### For New Cryptographic Code

- [ ] **Fuzz target for all crypto operations**:
  - [ ] Signing/verification
  - [ ] Encryption/decryption
  - [ ] Key derivation
  - [ ] Hashing
- [ ] **Round-trip testing** (where applicable):
  - [ ] encrypt → decrypt = original
  - [ ] sign → verify succeeds
- [ ] **Test vectors included** (if available):
  - [ ] RFC test vectors
  - [ ] Known good/bad inputs
- [ ] **Constant-time verification** (timing attacks):
  - [ ] Uses `ConstantTimeEq` for comparisons
  - [ ] No early returns based on secrets

### For New HTTP Endpoints

- [ ] **Fuzz target for request parsing**:
  - [ ] JSON body fuzzing
  - [ ] Query parameter fuzzing
  - [ ] Header fuzzing
- [ ] **Error handling tested**:
  - [ ] Malformed JSON returns 4xx (not 5xx)
  - [ ] Oversized payloads rejected
  - [ ] Content-Type validation
- [ ] **Input validation fuzzing**:
  - [ ] All string fields
  - [ ] All numeric fields (overflow)
  - [ ] All optional fields (None case)

### For Database Code

- [ ] **Fuzz target for SQL inputs**:
  - [ ] All string parameters
  - [ ] Special characters: `'`, `"`, `\`, `;`, `--`, `/*`
  - [ ] Unicode edge cases
- [ ] **Constraint handling**:
  - [ ] Foreign key violations
  - [ ] Unique constraint violations
  - [ ] Check constraint violations
- [ ] **No raw SQL concatenation**:
  - [ ] Always use parameterized queries (`$1`, `$2`)
  - [ ] Never `format!("SELECT ... WHERE id = {}", id)`

---

## Design Phase Checklist

Before implementing a new feature, answer these questions:

### Input Analysis

1. **What is the input source?**
   - [ ] Network (WebTransport, HTTP, WebSocket)
   - [ ] User (form data, file upload)
   - [ ] Database (query results)
   - [ ] Configuration file
   - [ ] Inter-service RPC

2. **Is the input trusted?**
   - [ ] Untrusted (external client)
   - [ ] Semi-trusted (authenticated user)
   - [ ] Trusted (internal service)
   - [ ] Fully trusted (local config)

3. **What parsing is required?**
   - [ ] Binary protocol (custom format)
   - [ ] Text protocol (HTTP, JSON)
   - [ ] Structured data (Protocol Buffers, TOML)
   - [ ] Cryptographic data (JWT, signatures)

### Fuzz Strategy

4. **Fuzz priority?** (based on above)
   - [ ] CRITICAL (network + binary/crypto)
   - [ ] HIGH (network + text, or user input)
   - [ ] MEDIUM (database, config)
   - [ ] LOW (internal, simple)
   - [ ] None (getters, pure functions)

5. **Fuzz technique?**
   - [ ] Raw bytes (`&[u8]` fuzzing)
   - [ ] Structured (`arbitrary::Arbitrary`)
   - [ ] Stateful (sequence of operations)
   - [ ] Round-trip (encode → decode)
   - [ ] Dictionary-guided (keywords)

6. **Success criteria?**
   - [ ] Execution count (10M for CRITICAL, 1M for HIGH)
   - [ ] Code coverage (90%+ for CRITICAL, 80%+ for HIGH)
   - [ ] Corpus size (1000+ for CRITICAL, 100+ for HIGH)

### Implementation Plan

7. **Deliverables:**
   - [ ] Implementation code
   - [ ] Unit tests (ADR-0005)
   - [ ] Integration tests (ADR-0005)
   - [ ] **Fuzz target** (ADR-0006) ← Don't forget!
   - [ ] Seed corpus
   - [ ] Documentation

---

## Examples from Dark Tower

### Example 1: Media Frame Codec ✅

**Input**: WebTransport datagram (network, binary, untrusted)

**Priority**: 🔴 CRITICAL

**Fuzz Targets**:
- `media_protocol/fuzz/fuzz_targets/codec_decode.rs` - Malformed input
- `media_protocol/fuzz/fuzz_targets/codec_roundtrip.rs` - Correctness

**Seed Corpus**:
- Valid audio frame
- Valid video keyframe
- Valid video delta frame
- Frames with all flags combinations
- Edge cases: max payload length, sequence overflow

**CI**: Runs for 60s per target on every PR

---

### Example 2: JWT Validation ✅

**Input**: HTTP Authorization header (network, text, untrusted)

**Priority**: 🔴 CRITICAL (authentication bypass risk)

**Fuzz Targets**:
- `ac_service/fuzz/fuzz_targets/jwt_validation.rs` - Malformed JWTs

**Structured Fuzzing**:
```rust
#[derive(Arbitrary)]
struct FuzzJwt {
    header: Vec<u8>,
    payload: Vec<u8>,
    signature: Vec<u8>,
}
```

**Seed Corpus**:
- Valid JWT (signed correctly)
- Expired JWT
- JWT with invalid signature
- JWT with tampered payload
- JWT missing parts

**Success Criteria**: 10M executions, 0 authentication bypasses

---

### Example 3: Meeting Configuration ✅

**Input**: JSON request body (HTTP, structured, semi-trusted)

**Priority**: 🟠 HIGH

**Fuzz Target**:
- `global_controller/fuzz/fuzz_targets/meeting_config.rs`

**Structured Fuzzing**:
```rust
#[derive(Arbitrary, Deserialize)]
struct FuzzMeetingConfig {
    max_participants: u32,
    enable_recording: bool,
    settings: HashMap<String, String>,
}
```

**Seed Corpus**:
- Valid config (default settings)
- Config with all optional fields
- Config with empty settings
- Config with large participant count

**Success Criteria**: 1M executions, 80%+ coverage

---

## Common Mistakes

### Mistake 1: Fuzzing with Valid Input Only

```rust
// ❌ BAD: Only tests valid inputs (not fuzzing)
#[derive(Arbitrary)]
struct FuzzInput {
    value: u32,  // Always valid u32
}

// ✅ GOOD: Tests arbitrary bytes
fuzz_target!(|data: &[u8]| {
    let _ = parse(data);  // Can be invalid
});
```

### Mistake 2: Panicking in Fuzz Target

```rust
// ❌ BAD: Panics if parsing fails (defeats purpose of fuzzing)
fuzz_target!(|data: &[u8]| {
    let parsed = parse(data).unwrap();  // PANICS!
    process(parsed);
});

// ✅ GOOD: Gracefully handles errors
fuzz_target!(|data: &[u8]| {
    if let Ok(parsed) = parse(data) {
        process(parsed);
    }
});
```

### Mistake 3: Expensive Operations in Fuzz Loop

```rust
// ❌ BAD: Database operations in fuzz loop (too slow)
fuzz_target!(|input: FuzzUser| {
    let pool = create_db_pool().await;  // Slow!
    insert_user(&pool, &input).await;   // Very slow!
});

// ✅ GOOD: Only test parsing/validation logic
fuzz_target!(|input: FuzzUser| {
    validate_username(&input.username)?;
    validate_email(&input.email)?;
    // Don't actually insert into DB
});
```

### Mistake 4: No Seed Corpus

```rust
// ❌ BAD: Empty corpus (fuzzer starts from nothing)
$ ls fuzz/corpus/target_name/
# (empty directory)

// ✅ GOOD: Seed with valid inputs
$ ls fuzz/corpus/target_name/
valid_input_1.bin
valid_input_2.bin
edge_case_1.bin
edge_case_2.bin
```

### Mistake 5: Ignoring Crashes

```bash
# ❌ BAD: Fuzzer found crash, but developer ignores it
$ cargo fuzz run target
# Crash found in fuzz/artifacts/target/crash-abc123
$ git push  # Pushes code with crash!

# ✅ GOOD: Fix crash before pushing
$ cargo fuzz tmin target fuzz/artifacts/target/crash-abc123
# Debug minimized crash
# Fix bug
# Add regression test
$ cargo fuzz run target -- -runs=1000000  # Verify fix
$ git push
```

---

## TL;DR - When to Fuzz

**Always fuzz**:
- Network data parsers
- Cryptographic operations
- Binary protocol codecs
- HTTP request handlers
- Database input validation

**Usually fuzz**:
- Complex deserialization
- Configuration parsing
- Internal protocols

**Sometimes fuzz**:
- Business logic algorithms
- State machines
- Data transformations

**Never fuzz**:
- Simple getters/setters
- Pure computation (trusted input)
- Logging/metrics

**Blocked from merge without fuzz target**:
- Network binary parsers
- Authentication/crypto code
- Anything marked 🔴 CRITICAL

---

## Quick Reference

| Code Pattern | Priority | Fuzz Target Type | Success Criteria |
|--------------|----------|------------------|------------------|
| Network binary parser | 🔴 CRITICAL | Raw bytes | 10M execs, 90% cov |
| Crypto operations | 🔴 CRITICAL | Round-trip | 10M execs, 100% cov |
| HTTP JSON handler | 🟠 HIGH | Structured | 1M execs, 80% cov |
| Database inputs | 🟠 HIGH | Structured | 1M execs, 80% cov |
| Config parsing | 🟡 MEDIUM | Structured | 100k execs, 70% cov |
| Business logic | 🟢 LOW | Optional | 10k execs, 50% cov |

---

**Questions?** See `docs/FUZZING.md` or ask in #engineering Slack.

**Found security issue?** Create private security advisory on GitHub.
