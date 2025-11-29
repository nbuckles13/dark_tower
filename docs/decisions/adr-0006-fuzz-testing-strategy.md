# ADR-0006: Fuzz Testing Strategy

**Status**: Proposed

**Date**: 2025-11-28

**Deciders**: Test Specialist

---

## Context

Dark Tower is a real-time video conferencing platform that processes untrusted input from multiple sources:

1. **Network Protocol Parsing**:
   - 42-byte media frame headers (custom binary protocol)
   - Protocol Buffer messages (signaling)
   - JWT tokens (authentication)
   - HTTP requests (REST APIs)

2. **Cryptographic Operations**:
   - EdDSA (Ed25519) JWT signing/verification
   - AES-256-GCM private key encryption
   - bcrypt password hashing
   - Key rotation logic

3. **Data Deserialization**:
   - PostgreSQL query results
   - Redis cache data
   - JSON API payloads
   - Binary media payloads

4. **Security-Critical Paths**:
   - Authentication flows
   - Authorization checks
   - Input validation
   - Database sanitization

**Current Testing Gap**: ADR-0005 established comprehensive testing with 90%+ coverage, but traditional tests use **valid, expected inputs**. We need **fuzz testing** to discover edge cases, malformed inputs, and security vulnerabilities that structured tests miss.

### Why Fuzz Testing for Dark Tower?

**Real-World Threats**:
- Malicious clients sending crafted media frames to crash Media Handlers
- Attackers fuzzing JWT parsers to bypass authentication
- Protocol Buffer deserialization bugs causing panics
- Integer overflow in media frame sequence numbers
- Buffer overflows in binary protocol parsing

**Specific Dark Tower Risks**:
1. **Media frame codec** (42-byte header): Untrusted network data, performance-critical
2. **JWT parsing**: Authentication bypass if parser has bugs
3. **Protocol Buffers**: Deserialization vulnerabilities
4. **Database inputs**: SQL injection, constraint violations
5. **WebTransport streams**: Malformed QUIC datagrams

## Decision

We adopt **cargo-fuzz with libFuzzer** as our primary fuzz testing framework, integrated into CI/CD with **continuous fuzzing** via OSS-Fuzz integration.

### Core Principles

1. **Security-Critical Code Gets 100% Fuzz Coverage**:
   - All parsers for untrusted input
   - All cryptographic operations
   - All authentication/authorization logic
   - All binary protocol codecs

2. **Fast Feedback via Continuous Fuzzing**:
   - Fuzz targets run in CI on every PR (time-boxed: 60s per target)
   - Long-running fuzzing in OSS-Fuzz (24/7, corpus-driven)
   - Developers can run fuzzers locally (cargo-fuzz)

3. **Corpus Management**:
   - Seed corpus with valid inputs from test suite
   - Automatically minimize corpus (smallest inputs that trigger bugs)
   - Share corpus between CI and OSS-Fuzz
   - Version-control corpus for reproducibility

4. **Integration with Existing Tests**:
   - Fuzz findings automatically converted to regression tests
   - Fuzzing complements ADR-0005 strategy (doesn't replace it)
   - Same coverage tools (cargo-llvm-cov) track fuzz coverage

## Mandatory Fuzz Targets

### 1. Media Frame Codec (CRITICAL)

**Why**: 42-byte binary protocol, untrusted network input, performance-critical

**Fuzz Target**: `crates/media-protocol/fuzz/fuzz_targets/codec_decode.rs`

```rust
#![no_main]
use libfuzzer_sys::fuzz_target;
use media_protocol::codec::decode_frame;
use bytes::Bytes;

fuzz_target!(|data: &[u8]| {
    let mut buf = Bytes::copy_from_slice(data);
    let _ = decode_frame(&mut buf);
    // Should never panic, only return Err on invalid input
});
```

**Properties to Verify**:
- ✅ No panics on malformed input
- ✅ No buffer overflows
- ✅ No integer overflows (payload length, sequence numbers)
- ✅ Properly rejects invalid versions
- ✅ Properly rejects invalid frame types
- ✅ Handles truncated headers/payloads gracefully

**Success Criteria**: 1 million executions without crashes, 90%+ code coverage

---

### 2. Protocol Buffer Deserialization (CRITICAL)

**Why**: Untrusted client signaling messages, potential deserialization vulnerabilities

**Fuzz Target**: `crates/proto-gen/fuzz/fuzz_targets/signaling_messages.rs`

```rust
#![no_main]
use libfuzzer_sys::fuzz_target;
use prost::Message;
use proto_gen::signaling::{ClientMessage, ServerMessage};

fuzz_target!(|data: &[u8]| {
    // Fuzz client messages (from untrusted clients)
    let _ = ClientMessage::decode(data);

    // Also fuzz server messages (internal consistency check)
    let _ = ServerMessage::decode(data);
});
```

**Properties to Verify**:
- ✅ No panics on malformed protobuf
- ✅ Handles missing required fields
- ✅ Handles oversized messages
- ✅ Validates enum values
- ✅ Properly handles UTF-8 validation

**Success Criteria**: 5 million executions without crashes

---

### 3. JWT Parsing and Validation (CRITICAL)

**Why**: Authentication bypass if parser has bugs

**Fuzz Target**: `crates/ac-service/fuzz/fuzz_targets/jwt_validation.rs`

```rust
#![no_main]
use libfuzzer_sys::fuzz_target;
use ac_service::crypto::jwt::{validate_token, JwtValidationError};

fuzz_target!(|data: &[u8]| {
    if let Ok(token) = std::str::from_utf8(data) {
        // Should gracefully reject invalid tokens
        let _ = validate_token(token, &public_key);
    }
});
```

**Structured Fuzzing** (deeper testing):

```rust
use arbitrary::Arbitrary;

#[derive(Arbitrary, Debug)]
struct FuzzJwt {
    header: Vec<u8>,
    payload: Vec<u8>,
    signature: Vec<u8>,
}

fuzz_target!(|jwt: FuzzJwt| {
    let token = format!(
        "{}.{}.{}",
        base64_url_encode(&jwt.header),
        base64_url_encode(&jwt.payload),
        base64_url_encode(&jwt.signature)
    );
    let _ = validate_token(&token, &public_key);
});
```

**Properties to Verify**:
- ✅ No panics on malformed JWTs
- ✅ Properly validates signature (no bypass)
- ✅ Rejects expired tokens
- ✅ Rejects tampered payloads
- ✅ Handles missing/extra JWT parts
- ✅ Validates base64 decoding errors

**Success Criteria**: 10 million executions, no authentication bypasses

---

### 4. Database Input Sanitization (HIGH)

**Why**: SQL injection, constraint violations

**Fuzz Target**: `crates/ac-service/fuzz/fuzz_targets/db_inputs.rs`

```rust
#![no_main]
use libfuzzer_sys::fuzz_target;
use ac_service::repositories::service_credentials;
use arbitrary::Arbitrary;

#[derive(Arbitrary, Debug)]
struct FuzzServiceCredential {
    client_id: String,
    client_secret: String,
    service_name: String,
    scopes: Vec<String>,
}

fuzz_target!(|cred: FuzzServiceCredential| {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let pool = runtime.block_on(setup_fuzz_db_pool());

    // Attempt to insert fuzzed credential
    let result = runtime.block_on(
        service_credentials::create_service_credential(
            &pool,
            &cred.client_id,
            &cred.client_secret,
            &cred.service_name,
            None,
            &cred.scopes,
        )
    );

    // Should either succeed or return proper error, never panic
    match result {
        Ok(_) => {}, // Valid input accepted
        Err(_) => {}, // Invalid input rejected gracefully
    }
});
```

**Properties to Verify**:
- ✅ No SQL injection possible
- ✅ Handles special characters in strings
- ✅ Validates UUID formats
- ✅ Enforces constraints (foreign keys, uniqueness)
- ✅ No panics on constraint violations

**Success Criteria**: 1 million executions, no SQL injections

---

### 5. HTTP Request Parsing (HIGH)

**Why**: Untrusted HTTP headers, query parameters, body payloads

**Fuzz Target**: `crates/ac-service/fuzz/fuzz_targets/http_handlers.rs`

```rust
#![no_main]
use libfuzzer_sys::fuzz_target;
use axum::{http::Request, body::Body};
use tower::ServiceExt;

fuzz_target!(|data: &[u8]| {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let app = runtime.block_on(create_test_app());

    // Attempt to parse as HTTP request
    if let Ok(req) = Request::builder()
        .uri("/api/v1/auth/service/token")
        .method("POST")
        .body(Body::from(data.to_vec()))
    {
        // Should handle any request body gracefully
        let _ = runtime.block_on(app.oneshot(req));
    }
});
```

**Properties to Verify**:
- ✅ No panics on malformed JSON
- ✅ Validates content-type headers
- ✅ Handles oversized payloads
- ✅ Proper error responses (not 500 Internal Server Error)

**Success Criteria**: 500k executions without crashes

---

### 6. AES-256-GCM Encryption/Decryption (MEDIUM)

**Why**: Private key encryption at rest, cryptographic correctness

**Fuzz Target**: `crates/ac-service/fuzz/fuzz_targets/aes_gcm.rs`

```rust
#![no_main]
use libfuzzer_sys::fuzz_target;
use ac_service::crypto::encryption::{encrypt_private_key, decrypt_private_key};

fuzz_target!(|data: &[u8]| {
    let master_key = b"test_master_key_32_bytes_long!!!";

    // Round-trip: encrypt then decrypt
    if let Ok(encrypted) = encrypt_private_key(data, master_key) {
        if let Ok(decrypted) = decrypt_private_key(&encrypted, master_key) {
            assert_eq!(&decrypted, data, "Decryption must restore original data");
        }
    }
});
```

**Properties to Verify**:
- ✅ Round-trip correctness (encrypt → decrypt = original)
- ✅ No panics on malformed ciphertext
- ✅ Properly handles authentication failures
- ✅ Nonce uniqueness verification

**Success Criteria**: 1 million executions, no crypto bugs

---

### 7. Media Frame Round-Trip Codec (MEDIUM)

**Why**: Ensures codec correctness under all inputs

**Fuzz Target**: `crates/media-protocol/fuzz/fuzz_targets/codec_roundtrip.rs`

```rust
#![no_main]
use libfuzzer_sys::fuzz_target;
use media_protocol::frame::{MediaFrame, FrameType, FrameFlags};
use media_protocol::codec::{encode_frame, decode_frame};
use bytes::Bytes;
use arbitrary::Arbitrary;

#[derive(Arbitrary, Debug)]
struct FuzzMediaFrame {
    user_id: u64,
    stream_id: u32,
    frame_type: u8, // Will map to FrameType
    timestamp: u64,
    sequence: u64,
    end_of_frame: bool,
    discardable: bool,
    payload: Vec<u8>,
}

fuzz_target!(|fuzz_frame: FuzzMediaFrame| {
    let frame_type = match fuzz_frame.frame_type % 3 {
        0 => FrameType::Audio,
        1 => FrameType::VideoKey,
        _ => FrameType::VideoDelta,
    };

    let frame = MediaFrame {
        version: MediaFrame::VERSION,
        user_id: fuzz_frame.user_id,
        stream_id: fuzz_frame.stream_id,
        frame_type,
        timestamp: fuzz_frame.timestamp,
        sequence: fuzz_frame.sequence,
        flags: FrameFlags {
            end_of_frame: fuzz_frame.end_of_frame,
            discardable: fuzz_frame.discardable,
        },
        payload: Bytes::from(fuzz_frame.payload),
    };

    // Encode
    let encoded = encode_frame(&frame).expect("Encoding should not fail for valid frame");

    // Decode
    let mut buf = encoded.clone();
    let decoded = decode_frame(&mut buf).expect("Decoding our own encoding should succeed");

    // Verify round-trip correctness
    assert_eq!(decoded.user_id, frame.user_id);
    assert_eq!(decoded.stream_id, frame.stream_id);
    assert_eq!(decoded.timestamp, frame.timestamp);
    assert_eq!(decoded.sequence, frame.sequence);
    assert_eq!(decoded.payload, frame.payload);
});
```

**Properties to Verify**:
- ✅ Round-trip correctness (encode → decode = original)
- ✅ No data loss
- ✅ Handles all valid frame types
- ✅ Preserves all header fields

**Success Criteria**: 5 million executions, perfect round-trip

## Fuzz Testing Workflow

### Local Development

**1. Install cargo-fuzz**:
```bash
cargo install cargo-fuzz
```

**2. Run a fuzz target**:
```bash
# Run media frame decoder fuzzer
cd crates/media-protocol
cargo fuzz run codec_decode

# Run for specific duration
cargo fuzz run codec_decode -- -max_total_time=60

# Run with specific number of iterations
cargo fuzz run codec_decode -- -runs=1000000
```

**3. Minimize a crashing input**:
```bash
# If fuzzer finds crash in fuzz/artifacts/codec_decode/crash-xxx
cargo fuzz cmin codec_decode

# Minimize specific crash
cargo fuzz tmin codec_decode fuzz/artifacts/codec_decode/crash-abc123
```

**4. Add crash as regression test**:
```bash
# Automatically generate test from crash
cargo fuzz coverage codec_decode
```

### CI/CD Integration

**GitHub Actions** (`.github/workflows/fuzz.yml`):

```yaml
name: Fuzz Testing

on:
  pull_request:
  schedule:
    - cron: '0 2 * * *'  # Nightly at 2 AM

jobs:
  fuzz:
    runs-on: ubuntu-latest
    strategy:
      matrix:
        target:
          - codec_decode
          - signaling_messages
          - jwt_validation
          - db_inputs
          - http_handlers
          - aes_gcm
          - codec_roundtrip

    steps:
      - uses: actions/checkout@v4

      - name: Install Rust nightly
        uses: actions-rust-lang/setup-rust-toolchain@v1
        with:
          toolchain: nightly

      - name: Install cargo-fuzz
        run: cargo install cargo-fuzz

      - name: Run fuzzer (60s per target)
        run: |
          cargo fuzz run ${{ matrix.target }} -- -max_total_time=60 -rss_limit_mb=2048
        continue-on-error: true

      - name: Check for crashes
        run: |
          if [ -d fuzz/artifacts/${{ matrix.target }} ]; then
            echo "::error::Fuzzer found crashes!"
            ls -la fuzz/artifacts/${{ matrix.target }}
            exit 1
          fi

      - name: Upload artifacts
        if: failure()
        uses: actions/upload-artifact@v3
        with:
          name: fuzz-artifacts-${{ matrix.target }}
          path: fuzz/artifacts/${{ matrix.target }}/
```

**PR Requirement**: All fuzz targets must run for 60 seconds without crashes before merge.

**Nightly Fuzzing**: Extended fuzzing runs for 8 hours nightly, uploads corpus to S3.

### OSS-Fuzz Integration (Future)

**Benefits**:
- 24/7 continuous fuzzing on Google's infrastructure
- Automatic bug reporting via email
- ClusterFuzz for crash triaging
- Public disclosure process

**Setup**:
1. Submit Dark Tower to OSS-Fuzz project
2. Create `oss-fuzz/dark-tower/` directory with build scripts
3. Configure corpus upload to Google Cloud Storage
4. Receive automated bug reports

**Timeline**: Phase 6 (post-MVP, when project is public)

## When to Add Fuzz Targets

### During Design Phase

**Trigger**: Designing a parser or codec for untrusted input

**Checklist**:
- [ ] Identify input source (network, file, database, user)
- [ ] Assess trust level (untrusted network > untrusted user > trusted service)
- [ ] Estimate attack surface (parsing complexity, buffer operations)
- [ ] Determine fuzz priority (CRITICAL > HIGH > MEDIUM > LOW)

**Decision Matrix**:

| Input Source | Trust Level | Parsing Complexity | Fuzz Priority |
|--------------|-------------|-------------------|---------------|
| Network (WebTransport) | Untrusted | High (binary protocol) | **CRITICAL** |
| HTTP API (JSON) | Untrusted | Medium | **HIGH** |
| Database results | Semi-trusted | Low | **MEDIUM** |
| Config files | Trusted | Low | **LOW** |

**Example**: Designing a new RTP media payload parser → **CRITICAL** fuzz target required before implementation.

### During Code Review

**Reviewer Checklist** (in addition to ADR-0005 test coverage):

**CRITICAL - Block merge if missing**:
- [ ] No fuzz target for parser accepting untrusted network data
- [ ] No fuzz target for cryptographic operations
- [ ] No fuzz target for authentication/authorization logic

**HIGH - Fix before merge**:
- [ ] No fuzz target for binary protocol codec
- [ ] No fuzz target for complex deserialization logic
- [ ] No fuzz target for input validation with security implications

**MEDIUM - Fix soon (next sprint)**:
- [ ] No fuzz target for database input sanitization
- [ ] No fuzz target for configuration parsing

**LOW - Nice to have**:
- [ ] Fuzz targets for internal APIs (trusted input)
- [ ] Fuzz targets for simple data structures

### Fuzz-Worthy Code Patterns

**Automatic Fuzz Target Required** (non-negotiable):

```rust
// ❌ CRITICAL: Parser for untrusted input, no fuzz target
fn parse_media_frame(data: &[u8]) -> Result<MediaFrame> {
    // Complex binary parsing from network
}

// ✅ Must have fuzz_media_frame.rs

// ❌ CRITICAL: Deserialization from untrusted source
fn deserialize_proto(data: &[u8]) -> Result<Message> {
    Message::decode(data)
}

// ✅ Must have fuzz_proto_deserialize.rs

// ❌ HIGH: Authentication logic
fn validate_jwt(token: &str) -> Result<Claims> {
    // JWT parsing and validation
}

// ✅ Must have fuzz_jwt_validation.rs
```

**Strongly Recommended**:

```rust
// Database input handling
fn insert_user(db: &PgPool, username: &str) -> Result<()> {
    sqlx::query("INSERT INTO users (username) VALUES ($1)")
        .bind(username)
        .execute(db)
        .await
}

// ✅ Should have fuzz_db_inputs.rs to test SQL injection resistance
```

**Optional** (but helpful):

```rust
// Internal data structure transformations
fn convert_layout(layout: LayoutConfig) -> GridLayout {
    // Simple conversion logic
}

// ⚠️ Optional: fuzz_layout_conversion.rs if complex
```

## Fuzz Testing Success Criteria

### Per-Target Criteria

**CRITICAL Targets** (media codec, JWT, protobuf):
- ✅ 10 million executions without crashes
- ✅ 90%+ code coverage of target module
- ✅ No memory leaks (valgrind/ASAN clean)
- ✅ Corpus size > 1000 inputs
- ✅ Daily fuzzing in CI (nightly builds)

**HIGH Targets** (HTTP parsing, database inputs):
- ✅ 1 million executions without crashes
- ✅ 80%+ code coverage
- ✅ Corpus size > 500 inputs
- ✅ Weekly fuzzing in CI

**MEDIUM Targets** (crypto round-trip, internal APIs):
- ✅ 100k executions without crashes
- ✅ 70%+ code coverage
- ✅ Corpus size > 100 inputs
- ✅ On-demand fuzzing (manual runs)

### Continuous Fuzzing Metrics

**Track in CI dashboard**:
- Total executions per target (cumulative)
- Code coverage per target
- Crashes discovered (0 is goal)
- Corpus growth over time
- Fuzzing duration per week

**Alert thresholds**:
- 🔴 **CRITICAL**: New crash discovered → immediate Slack alert, block deploys
- 🟠 **HIGH**: Coverage dropped >5% → investigate regression
- 🟡 **MEDIUM**: Corpus not growing (stale fuzzer) → review target effectiveness

## Fuzz Testing vs. Traditional Testing

**Complementary, Not Replacement**:

| Test Type | Purpose | Input Strategy | Dark Tower Example |
|-----------|---------|----------------|-------------------|
| **Unit Tests** | Verify expected behavior | Valid inputs | `test_decode_valid_media_frame()` |
| **Integration Tests** | Multi-component interaction | Valid scenarios | `test_jwt_validation_flow()` |
| **E2E Tests** | User journeys | Realistic data | `test_meeting_lifecycle()` |
| **Fuzz Tests** | Find unexpected crashes | Random/malformed | `fuzz_codec_decode()` |
| **Property Tests** | Invariants hold | Generated valid data | `proptest_roundtrip_codec()` |

**Workflow Integration**:

1. **Development**: Write unit tests (ADR-0005) + fuzz target
2. **Code Review**: Verify test coverage + fuzz target exists
3. **CI**: Run unit/integration/E2E + 60s fuzz per target
4. **Nightly**: Extended fuzzing (8 hours) + corpus update
5. **Bug Discovery**: Add crash as regression test
6. **Release**: All fuzzers must pass (0 crashes)

## Implementation Checklist

### Phase 1: Infrastructure Setup (Days 1-3)

- [ ] Install cargo-fuzz tooling
- [ ] Create fuzz directory structure per crate:
  ```
  crates/media-protocol/
    fuzz/
      Cargo.toml
      fuzz_targets/
        codec_decode.rs
        codec_roundtrip.rs
  ```
- [ ] Configure CI workflow (`.github/workflows/fuzz.yml`)
- [ ] Set up corpus storage (Git LFS or S3)
- [ ] Create `docs/FUZZING.md` developer guide

### Phase 2: CRITICAL Fuzz Targets (Days 4-10)

- [ ] **Media frame codec** (`codec_decode.rs`, `codec_roundtrip.rs`)
  - [ ] Seed corpus with valid frames from tests
  - [ ] Run 10M iterations
  - [ ] Verify 90%+ coverage
- [ ] **Protocol Buffer deserialization** (`signaling_messages.rs`)
  - [ ] Seed corpus with valid protobufs
  - [ ] Run 5M iterations
  - [ ] Test all message types
- [ ] **JWT validation** (`jwt_validation.rs`)
  - [ ] Seed corpus with valid/invalid tokens
  - [ ] Run 10M iterations
  - [ ] Verify no authentication bypass

### Phase 3: HIGH Fuzz Targets (Days 11-15)

- [ ] **Database inputs** (`db_inputs.rs`)
  - [ ] Test all repository functions
  - [ ] Verify SQL injection resistance
- [ ] **HTTP handlers** (`http_handlers.rs`)
  - [ ] Test all API endpoints
  - [ ] Verify error handling

### Phase 4: MEDIUM Fuzz Targets (Days 16-18)

- [ ] **AES-256-GCM encryption** (`aes_gcm.rs`)
  - [ ] Round-trip testing
  - [ ] Verify cryptographic correctness

### Phase 5: CI/CD Integration (Days 19-21)

- [ ] Add fuzz targets to CI pipeline
- [ ] Configure time limits (60s per target in PR, 8h nightly)
- [ ] Set up crash artifact upload
- [ ] Create Slack alerts for crashes
- [ ] Document fuzzing workflow in `CONTRIBUTING.md`

### Phase 6: Continuous Improvement (Ongoing)

- [ ] Review corpus monthly, remove redundant inputs
- [ ] Add fuzz targets for new parsers
- [ ] Track fuzzing metrics (executions, coverage)
- [ ] Consider OSS-Fuzz integration (when project is public)

**Total Estimated Effort**: 21 days (3 weeks) for full implementation

## Developer Workflow

### Adding a New Parser

**Example**: Implementing new RTCP media statistics parser

**Step 1: Design with fuzzing in mind**
```rust
// Design API with Result return (never panic on invalid input)
pub fn parse_rtcp_stats(data: &[u8]) -> Result<RtcpStats, ParseError> {
    // Implementation...
}
```

**Step 2: Write unit tests** (ADR-0005)
```rust
#[test]
fn test_parse_valid_rtcp_stats() {
    let data = &[/* valid RTCP packet */];
    let stats = parse_rtcp_stats(data).unwrap();
    assert_eq!(stats.packet_loss, 0);
}
```

**Step 3: Write fuzz target**
```rust
// crates/media-protocol/fuzz/fuzz_targets/rtcp_stats.rs
#![no_main]
use libfuzzer_sys::fuzz_target;
use media_protocol::rtcp::parse_rtcp_stats;

fuzz_target!(|data: &[u8]| {
    let _ = parse_rtcp_stats(data);
    // Should never panic
});
```

**Step 4: Run locally**
```bash
cd crates/media-protocol
cargo fuzz run rtcp_stats -- -max_total_time=300  # 5 minutes
```

**Step 5: Add to CI**
```yaml
# .github/workflows/fuzz.yml
strategy:
  matrix:
    target:
      - rtcp_stats  # Add new target
```

**Step 6: Code review**
- Reviewer verifies fuzz target exists
- Reviewer checks no panics in error paths
- CI runs fuzzer for 60s

### Handling Fuzz Crashes

**Scenario**: Fuzzer finds crash in JWT validation

**Step 1: Reproduce locally**
```bash
# Crash artifact saved to fuzz/artifacts/jwt_validation/crash-abc123
cargo fuzz run jwt_validation fuzz/artifacts/jwt_validation/crash-abc123
```

**Step 2: Minimize crashing input**
```bash
cargo fuzz tmin jwt_validation fuzz/artifacts/jwt_validation/crash-abc123
# Produces minimized input in fuzz/artifacts/jwt_validation/minimized-abc123
```

**Step 3: Debug**
```bash
# Run with debugger
rust-lldb target/x86_64-unknown-linux-gnu/release/jwt_validation \
  fuzz/artifacts/jwt_validation/minimized-abc123
```

**Step 4: Fix bug**
```rust
// Fix panic in jwt.rs
pub fn validate_token(token: &str) -> Result<Claims> {
    // Before: token.split('.').collect()[1] - panics if < 2 parts
    // After: Proper error handling
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 {
        return Err(JwtError::InvalidFormat);
    }
    // ...
}
```

**Step 5: Add regression test**
```rust
#[test]
fn test_jwt_with_missing_parts() {
    let result = validate_token("header.payload"); // Missing signature
    assert!(matches!(result, Err(JwtError::InvalidFormat)));
}
```

**Step 6: Re-run fuzzer**
```bash
# Verify crash is fixed
cargo fuzz run jwt_validation -- -runs=10000000
```

**Step 7: Update corpus**
```bash
# Add minimized crash to corpus (if unique)
cp fuzz/artifacts/jwt_validation/minimized-abc123 \
   fuzz/corpus/jwt_validation/
```

## Security Considerations

### Fuzzing as Security Testing

**Fuzz testing discovers**:
- ✅ Buffer overflows
- ✅ Integer overflows
- ✅ Unhandled edge cases
- ✅ Denial-of-service vulnerabilities
- ✅ Parser bugs (potential for code execution)

**Fuzz testing does NOT discover**:
- ❌ Business logic flaws
- ❌ Authorization bypasses (unless via parser bug)
- ❌ Race conditions (use ThreadSanitizer)
- ❌ Timing side-channels (use ConstantTimeEq)

### Sanitizer Integration

**Run fuzzers with sanitizers** for deeper bug detection:

**AddressSanitizer (ASAN)** - Memory safety:
```bash
export RUSTFLAGS="-Zsanitizer=address"
cargo +nightly fuzz run codec_decode
```

**UndefinedBehaviorSanitizer (UBSAN)** - Undefined behavior:
```bash
export RUSTFLAGS="-Zsanitizer=undefined"
cargo +nightly fuzz run codec_decode
```

**MemorySanitizer (MSAN)** - Uninitialized memory:
```bash
export RUSTFLAGS="-Zsanitizer=memory"
cargo +nightly fuzz run codec_decode
```

**CI Configuration**: Nightly builds run fuzzers with ASAN enabled.

### Responsible Disclosure

**If fuzzer finds security vulnerability**:

1. **Assess severity** (CVSS scoring)
2. **Create private security advisory** (GitHub Security Advisories)
3. **Fix vulnerability** in private branch
4. **Write regression test** (without revealing exploit details)
5. **Release patch** with CVE if applicable
6. **Public disclosure** after users have time to update

**Example**: Fuzzer finds buffer overflow in media frame parser
- Severity: CRITICAL (remote code execution)
- Fix: Add bounds check before buffer access
- Regression test: Add test with oversized payload length
- Patch: Release v0.2.1 with security fix
- Disclosure: Publish CVE 30 days after patch

## Metrics and Monitoring

### Fuzzing Dashboard (Grafana)

**Metrics to track**:
- Total executions per target (gauge)
- Executions per second (rate)
- Code coverage percentage (gauge)
- Crashes discovered (counter)
- Corpus size (gauge)
- Fuzzing time budget used (%)

**Alerts**:
- 🔴 New crash discovered → PagerDuty
- 🟠 Coverage dropped >5% → Slack
- 🟡 Fuzzer hanging (0 exec/s for 5min) → Email

### Weekly Fuzzing Report

**Automated report** (sent to #engineering Slack):

```
Weekly Fuzzing Report - 2025-11-28

Targets Fuzzed: 7
Total Executions: 156,234,890
Crashes Found: 0 ✅
Coverage:
  codec_decode: 94% ✅
  jwt_validation: 98% ✅
  signaling_messages: 87% ⚠️ (down from 91%)

Action Items:
- Investigate coverage drop in signaling_messages
- codec_roundtrip corpus not growing (add diverse inputs)
```

## Alternatives Considered

### Alternative 1: AFL++ Instead of libFuzzer

**Pros**:
- Better coverage-guided fuzzing
- QEMU mode for binary fuzzing
- More mature (20+ years)

**Cons**:
- Harder integration with Rust
- cargo-fuzz uses libFuzzer by default
- Less developer-friendly

**Decision**: Use libFuzzer via cargo-fuzz for simplicity. Reconsider AFL++ if libFuzzer proves insufficient.

### Alternative 2: Property-Based Testing (proptest) Only

**Pros**:
- Integrated with Rust testing
- No separate tooling
- Generates valid structured inputs

**Cons**:
- Less effective at finding crashes than fuzzing
- Requires writing generators (more upfront work)
- Not continuous (only runs in CI tests)

**Decision**: Use both. Property tests for business logic invariants, fuzz tests for crash discovery.

### Alternative 3: No Fuzz Testing (Manual Security Review Only)

**Pros**:
- No tooling overhead
- Faster development

**Cons**:
- Manual review misses edge cases
- No continuous security testing
- High risk for security vulnerabilities

**Decision**: Rejected. Dark Tower handles untrusted network input - fuzzing is essential.

### Alternative 4: Only Fuzz in Production (Chaos Engineering)

**Pros**:
- Tests real production environment
- Finds issues under actual load

**Cons**:
- Too risky (could crash production)
- Late feedback (after deployment)
- Can't test destructive scenarios

**Decision**: Rejected. Fuzz in CI/CD, use chaos engineering separately for resilience testing.

## References

- **ADR-0005**: Integration and End-to-End Testing Strategy (complementary to fuzzing)
- **ADR-0002**: No-Panic Policy (fuzz targets verify this)
- **cargo-fuzz**: https://rust-fuzz.github.io/book/cargo-fuzz.html
- **libFuzzer**: https://llvm.org/docs/LibFuzzer.html
- **OSS-Fuzz**: https://google.github.io/oss-fuzz/
- **Rust Fuzz Book**: https://rust-fuzz.github.io/book/
- **Arbitrary crate**: https://docs.rs/arbitrary/ (structured fuzzing)

---

## Conclusion

Fuzz testing is **mandatory** for Dark Tower's security-critical parsers and codecs. By integrating cargo-fuzz into our CI/CD pipeline and running continuous fuzzing, we significantly reduce the attack surface and discover crashes before they reach production.

**Key Takeaways**:
1. **All parsers for untrusted input must have fuzz targets** (non-negotiable)
2. **Fuzzing complements traditional testing** (ADR-0005), doesn't replace it
3. **Fast feedback via CI** (60s per target) catches bugs early
4. **Continuous fuzzing** (nightly, OSS-Fuzz) finds deep bugs over time
5. **Crashes become regression tests**, preventing reintroduction

**Success Metrics**:
- 0 crashes in production from fuzzed code paths
- 90%+ code coverage in CRITICAL fuzz targets
- 100% of parsers have fuzz targets by Phase 6

**Next Steps**: Implement Phase 1 infrastructure (3 days), then systematically add fuzz targets for all security-critical code.
