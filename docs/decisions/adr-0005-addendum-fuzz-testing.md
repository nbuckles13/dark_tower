# ADR-0005 Addendum: Integrating Fuzz Testing

**Status**: Proposed (Addendum to ADR-0005)

**Date**: 2025-11-28

**Parent ADR**: ADR-0005 (Integration and End-to-End Testing Strategy)

---

## Purpose

This addendum integrates **fuzz testing requirements** (from ADR-0006) into the existing testing strategy (ADR-0005). It clarifies when fuzz tests are required, how they complement existing test tiers, and updates coverage targets.

## Summary of Changes

ADR-0005 established three test tiers:
1. **Unit Tests** (in `#[cfg(test)]` modules)
2. **Integration Tests** (in `tests/integration/`)
3. **End-to-End Tests** (in `tests/e2e/`)

This addendum adds a **fourth tier**:

4. **Fuzz Tests** (in `fuzz/fuzz_targets/`)

## Updated Test Strategy

### Four-Tier Testing Model

| Test Tier | Purpose | Input Strategy | Location | When Required |
|-----------|---------|----------------|----------|---------------|
| **Unit** | Verify expected behavior | Valid inputs | `src/**/tests.rs` | All code |
| **Integration** | Multi-component interaction | Valid scenarios | `tests/integration/` | Services, repos |
| **E2E** | User journeys | Realistic data | `tests/e2e/` | Critical paths |
| **Fuzz** | Find crashes & edge cases | Malformed/random | `fuzz/fuzz_targets/` | **Parsers, crypto** |

### Updated Test Coverage Targets

**ADR-0005 Original Targets**:
- Crypto: 100%
- Handlers/Services/Repos: 95%
- Overall: 90%

**ADR-0006 Fuzz Targets (New)**:

| Module Type | Required Fuzz Coverage | Execution Count | Status |
|-------------|----------------------|-----------------|--------|
| **Network Parsers** | **100%** | 10M executions | **MANDATORY** |
| **Crypto Operations** | **100%** | 10M executions | **MANDATORY** |
| **Binary Codecs** | **95%** | 5M executions | **MANDATORY** |
| **HTTP Handlers** | **80%** | 1M executions | **REQUIRED** |
| **Database Inputs** | **80%** | 1M executions | **REQUIRED** |
| **Config Parsers** | **70%** | 100k executions | **RECOMMENDED** |

**Combined Coverage Goal**: 90%+ traditional coverage + 100% fuzz coverage for parsers/crypto

## Integration with ADR-0005 Checklist

### Updated Phase 4 Implementation

**ADR-0005 Phase 4.2** originally included:
- `crypto_fixtures.rs`
- `token_builders.rs`
- `server_harness.rs`
- `assertions.rs`

**Add to Phase 4.2** (Fuzz Test Utilities):
- [ ] Install `cargo-fuzz` tooling
- [ ] Create fuzz directory structure: `fuzz/fuzz_targets/`, `fuzz/corpus/`
- [ ] Create fuzz seed corpus from existing test fixtures
- [ ] Add fuzz targets to CI workflow (`.github/workflows/fuzz.yml`)

**New Phase 4.8** (Fuzz Testing):
- [ ] **CRITICAL Fuzz Targets** (mandatory for merge):
  - [ ] `codec_decode.rs` - Media frame decoder
  - [ ] `codec_roundtrip.rs` - Media frame encode/decode correctness
  - [ ] `jwt_validation.rs` - JWT parser and signature verification
  - [ ] `signaling_messages.rs` - Protocol Buffer deserialization
- [ ] **HIGH Fuzz Targets** (required before release):
  - [ ] `http_handlers.rs` - HTTP request parsing
  - [ ] `db_inputs.rs` - Database input sanitization
- [ ] **Seed Corpora**:
  - [ ] Media frame corpus (valid frames from integration tests)
  - [ ] JWT corpus (valid/expired/tampered tokens)
  - [ ] Protobuf corpus (valid signaling messages)
- [ ] **CI Integration**:
  - [ ] 60s fuzz per target on PR
  - [ ] 8-hour nightly fuzz runs
  - [ ] Crash artifact upload
  - [ ] Slack alerts for crashes

**Estimated Effort**: +7 days to Phase 4 (total: 22-24 days)

## Updated Test Tiers Boundaries

### Original ADR-0005 Boundaries

| Test Type | Scope | DB | HTTP | External Services |
|-----------|-------|----|----|-------------------|
| Unit | Single function | Mock/None | No | Mock |
| Integration | Single service layer | Real PostgreSQL | Tower ServiceExt | Mock |
| E2E | Full AC stack | Real PostgreSQL | Real server | Mock |

### Updated with Fuzz Testing

| Test Type | Scope | DB | HTTP | Input Type | Coverage Target |
|-----------|-------|----|----|-----------|----------------|
| Unit | Single function | Mock | No | Valid | 90% code |
| Integration | Service layer | Real | ServiceExt | Valid scenarios | 95% code |
| E2E | Full stack | Real | Real | Realistic | 90% code |
| **Fuzz** | **Parser/codec** | **None** | **No** | **Random/malformed** | **100% parser** |

**Key Difference**: Fuzz tests focus on **invalid inputs** that traditional tests never exercise.

## When Fuzz Tests are Required

### Code Review Gate (Updated)

**ADR-0005 Original**: Code must have unit + integration + E2E tests

**Updated with ADR-0006**:

**Block merge if missing** (🔴 CRITICAL):
- [ ] Unit tests (ADR-0005)
- [ ] Integration tests (ADR-0005)
- [ ] E2E tests for critical paths (ADR-0005)
- [ ] **Fuzz targets for network parsers** (NEW)
- [ ] **Fuzz targets for crypto operations** (NEW)

**Fix before release** (🟠 HIGH):
- [ ] Coverage ≥ 90% overall (ADR-0005)
- [ ] **Fuzz targets for HTTP handlers** (NEW)
- [ ] **Fuzz targets for database inputs** (NEW)

### Updated Test Naming Convention

**ADR-0005 Convention**: `test_<function>_<scenario>_<expected_result>`

**Add for Fuzz Targets**: `fuzz_<function>_<input_type>`

**Examples**:
- `fuzz_codec_decode_raw_bytes` - Fuzz media frame decoder with raw bytes
- `fuzz_jwt_validation_structured` - Fuzz JWT parser with structured input
- `fuzz_http_handler_json_body` - Fuzz HTTP endpoint with malformed JSON

## Updated CI/CD Configuration

### ADR-0005 Original CI Time Budget

| Test Tier | Target | Timeout |
|-----------|--------|---------|
| Unit | <1s | 10s |
| Integration | <5s | 30s |
| E2E | <30s | 2min |
| **Total** | **<2min** | **5min** |

### Updated with Fuzz Testing

| Test Tier | Target (PR) | Target (Nightly) | Timeout |
|-----------|-------------|------------------|---------|
| Unit | <1s | N/A | 10s |
| Integration | <5s | N/A | 30s |
| E2E | <30s | N/A | 2min |
| **Fuzz (per target)** | **60s** | **8 hours** | **5min (PR)** |
| **Total** | **<10min** | **N/A** | **30min** |

**Note**: Fuzz tests run in parallel matrix (7 targets × 60s = 7min wall clock if parallelized)

### Updated GitHub Actions Workflow

```yaml
# .github/workflows/ci.yml (existing from ADR-0005)
name: CI Tests

on: [push, pull_request]

jobs:
  test:
    runs-on: ubuntu-latest
    services:
      postgres: # ... (unchanged)

    steps:
      # ... (existing unit/integration/E2E steps)

      - name: Run unit + integration tests
        run: cargo test --workspace --lib --tests

      - name: Run E2E tests
        run: cargo test --test 'e2e_*'

      - name: Generate coverage
        run: cargo llvm-cov --workspace --lcov --output-path lcov.info

      # NEW: Fuzz testing
      - name: Run fuzz tests (quick check)
        uses: ./.github/workflows/fuzz.yml
        with:
          duration: 60  # 60 seconds per target
```

```yaml
# .github/workflows/fuzz.yml (NEW)
name: Fuzz Tests

on:
  pull_request:
  schedule:
    - cron: '0 2 * * *'  # Nightly 8-hour runs

jobs:
  fuzz:
    runs-on: ubuntu-latest
    strategy:
      matrix:
        target:
          - codec_decode
          - codec_roundtrip
          - jwt_validation
          - signaling_messages
          - http_handlers
          - db_inputs
          - aes_gcm

    steps:
      - uses: actions/checkout@v4

      - name: Install Rust nightly
        uses: actions-rust-lang/setup-rust-toolchain@v1
        with:
          toolchain: nightly

      - name: Install cargo-fuzz
        run: cargo install cargo-fuzz

      - name: Restore corpus from cache
        uses: actions/cache@v3
        with:
          path: fuzz/corpus
          key: fuzz-corpus-${{ matrix.target }}-${{ github.sha }}
          restore-keys: |
            fuzz-corpus-${{ matrix.target }}-

      - name: Run fuzzer (60s on PR, 8h nightly)
        run: |
          if [ "${{ github.event_name }}" = "schedule" ]; then
            DURATION=28800  # 8 hours
          else
            DURATION=60  # 60 seconds
          fi
          cargo fuzz run ${{ matrix.target }} -- \
            -max_total_time=$DURATION \
            -rss_limit_mb=2048

      - name: Check for crashes
        run: |
          if [ -d fuzz/artifacts/${{ matrix.target }} ]; then
            echo "::error::Fuzzer found crashes!"
            ls -la fuzz/artifacts/${{ matrix.target }}
            exit 1
          fi

      - name: Upload crash artifacts
        if: failure()
        uses: actions/upload-artifact@v3
        with:
          name: fuzz-${{ matrix.target }}-crashes
          path: fuzz/artifacts/${{ matrix.target }}/

      - name: Save updated corpus
        uses: actions/cache@v3
        with:
          path: fuzz/corpus/${{ matrix.target }}
          key: fuzz-corpus-${{ matrix.target }}-${{ github.sha }}
```

## Updated Test Utilities (ac-test-utils)

### ADR-0005 Original Utilities

```
crates/ac-test-utils/src/
  crypto_fixtures.rs    # Deterministic keys
  token_builders.rs     # TestTokenBuilder
  server_harness.rs     # TestAuthServer
  test_ids.rs           # Fixed UUIDs
  assertions.rs         # TokenAssertions trait
```

### New Fuzz Utilities (ADD)

```
crates/ac-test-utils/src/
  fuzz_fixtures.rs      # Fuzz-specific helpers
```

**New file: `fuzz_fixtures.rs`**:

```rust
/// Helper to create seed corpus from integration test fixtures
pub fn create_seed_corpus_from_tests(
    test_data_dir: &Path,
    corpus_dir: &Path,
) -> Result<()> {
    for entry in fs::read_dir(test_data_dir)? {
        let path = entry?.path();
        if path.extension() == Some("bin") {
            let dest = corpus_dir.join(path.file_name().unwrap());
            fs::copy(&path, &dest)?;
        }
    }
    Ok(())
}

/// Generate deterministic fuzz corpus for media frames
pub fn generate_media_frame_corpus() -> Vec<Vec<u8>> {
    let mut corpus = Vec::new();

    // Valid frames
    for frame_type in [FrameType::Audio, FrameType::VideoKey, FrameType::VideoDelta] {
        let frame = MediaFrame {
            version: MediaFrame::VERSION,
            user_id: 1,
            stream_id: 1,
            frame_type,
            timestamp: 1000,
            sequence: 1,
            flags: FrameFlags::default(),
            payload: Bytes::from(vec![0xAA; 100]),
        };
        corpus.push(encode_frame(&frame).unwrap().to_vec());
    }

    // Edge cases
    corpus.push(vec![]);  // Empty
    corpus.push(vec![0; 41]);  // Truncated header
    corpus.push(vec![0xFF; 42]);  // Invalid header
    corpus.push(vec![MediaFrame::VERSION, 0xFF]);  // Invalid frame type

    corpus
}
```

## Updated Documentation

### ADR-0005 Referenced Docs

| Document | Purpose |
|----------|---------|
| `tests/README.md` | Test suite overview |
| `tests/CONVENTIONS.md` | Naming/style guide |
| `ac-test-utils/README.md` | Utilities API |

### New Fuzz Testing Docs (ADD)

| Document | Purpose | Status |
|----------|---------|--------|
| `docs/FUZZING.md` | Developer fuzz guide | ✅ Created |
| `docs/FUZZ_THIS_CHECKLIST.md` | Code review checklist | ✅ Created |
| `fuzz/README.md` | Fuzz targets overview | TODO |

**New file: `fuzz/README.md`**:

```markdown
# Fuzz Targets for Dark Tower

This directory contains fuzz targets for security-critical code.

## Available Targets

- `codec_decode` - Media frame decoder (42-byte header)
- `codec_roundtrip` - Media frame encode/decode correctness
- `jwt_validation` - JWT parser and signature verification
- `signaling_messages` - Protocol Buffer deserialization
- `http_handlers` - HTTP request parsing
- `db_inputs` - Database input sanitization
- `aes_gcm` - AES-256-GCM encryption/decryption

## Quick Start

See `docs/FUZZING.md` for detailed guide.

## Corpus

Seed corpus stored in `corpus/<target_name>/`.
Generated from integration test fixtures + edge cases.
```

## Consequences

### Positive (Additional to ADR-0005)

1. **Earlier Bug Discovery**:
   - Fuzz tests find crashes before code review
   - Random inputs discover edge cases missed by structured tests
   - 24/7 continuous fuzzing finds bugs over time

2. **Security Hardening**:
   - 100% fuzz coverage for parsers prevents exploitation
   - Crypto fuzzing verifies correctness under all inputs
   - DoS resistance via crash testing

3. **Confidence in Robustness**:
   - Code survives 10M+ random inputs
   - No panics on malformed network data
   - Graceful error handling verified

### Negative (Additional to ADR-0005)

1. **Increased CI Time**:
   - ADR-0005 target: 2min total
   - With fuzzing: 10min total (7 targets × 60s + traditional tests)
   - Mitigation: Parallel execution, nightly extended runs

2. **Learning Curve**:
   - Developers must learn cargo-fuzz
   - Different mindset (invalid inputs vs. valid scenarios)
   - Mitigation: `docs/FUZZING.md` guide, examples

3. **Corpus Management**:
   - Git LFS or S3 for large corpora
   - Periodic minimization required
   - Mitigation: Automated corpus management in CI

### Neutral

1. **Complementary to ADR-0005**:
   - Fuzz tests don't replace traditional tests
   - Both required for comprehensive coverage
   - Different bug classes discovered

## Updated Success Criteria

**ADR-0005 Original Criteria**:
- ✅ 90%+ code coverage
- ✅ 100% coverage for crypto
- ✅ Token issuance p99 < 50ms
- ✅ All tests pass in CI < 2 minutes
- ✅ Zero flaky tests

**ADR-0006 Additional Criteria**:
- ✅ **100% fuzz coverage for network parsers**
- ✅ **10M+ executions for CRITICAL fuzz targets**
- ✅ **0 crashes in fuzzed code paths**
- ✅ **All fuzz targets pass in CI < 10 minutes**
- ✅ **Corpus growth tracked (not stagnant)**

## Migration Path

### Existing Code (AC Service Phase 3)

**Current State** (as of Phase 3):
- ✅ Unit tests for crypto (100% coverage)
- ❌ No integration tests yet
- ❌ No E2E tests yet
- ❌ **No fuzz tests**

**Phase 4 Plan** (with this addendum):

**Days 1-17**: Implement ADR-0005 (unchanged)
- Test infrastructure
- Test utilities
- Integration tests
- E2E tests
- Coverage + CI

**Days 18-24**: Add ADR-0006 fuzz testing (NEW)
- Day 18: Install cargo-fuzz, create directory structure
- Day 19-20: CRITICAL fuzz targets (codec, JWT, protobuf)
- Day 21-22: HIGH fuzz targets (HTTP, database)
- Day 23: CI integration (fuzz workflow)
- Day 24: Seed corpora, documentation

**Total**: 24 days (up from 17 days)

### New Code (Future Services)

**GC/MC/MH Implementation**:
- Design phase: Identify fuzz targets using `FUZZ_THIS_CHECKLIST.md`
- Implementation: Write fuzz target alongside code
- Code review: Verify fuzz target exists (gate merge)
- CI: Run fuzz tests on every PR

## Updated Files/Components

**ADR-0005 Original Files**:
- `crates/ac-test-utils/` (test utilities)
- `tests/integration/` (integration tests)
- `tests/e2e/` (E2E tests)
- `.github/workflows/ci.yml` (CI config)
- `.codecov.yml` (coverage config)

**ADR-0006 New Files (ADD)**:
- `fuzz/fuzz_targets/` (fuzz targets) - NEW
- `fuzz/corpus/` (seed corpus) - NEW
- `.github/workflows/fuzz.yml` (fuzz CI) - NEW
- `docs/FUZZING.md` (developer guide) - NEW
- `docs/FUZZ_THIS_CHECKLIST.md` (review checklist) - NEW
- `crates/ac-test-utils/src/fuzz_fixtures.rs` (fuzz utilities) - NEW

## Summary

This addendum integrates fuzz testing (ADR-0006) into the existing testing strategy (ADR-0005):

1. **Adds 4th test tier**: Fuzz tests complement unit/integration/E2E
2. **Updates coverage targets**: 100% fuzz coverage for parsers/crypto
3. **Extends CI time**: From 2min to 10min (acceptable trade-off for security)
4. **Adds Phase 4.8**: Fuzz test implementation (+7 days)
5. **Updates checklist**: Code review must verify fuzz targets exist
6. **New documentation**: `FUZZING.md`, `FUZZ_THIS_CHECKLIST.md`

**Key Principle**: Traditional tests verify **expected behavior**, fuzz tests discover **unexpected crashes**.

**Next Steps**: Implement Phase 4.8 after completing ADR-0005 Phases 4.1-4.7.

## References

- **ADR-0005**: Integration and End-to-End Testing Strategy (parent)
- **ADR-0006**: Fuzz Testing Strategy (full specification)
- **ADR-0002**: No-Panic Policy (fuzz tests verify compliance)
- **ADR-0003**: Service Authentication (crypto fuzzing required)
- `docs/FUZZING.md`: Developer guide for fuzz testing
- `docs/FUZZ_THIS_CHECKLIST.md`: Code review checklist
