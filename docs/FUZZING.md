# Fuzz Testing Guide for Dark Tower

This guide provides practical workflows for writing, running, and debugging fuzz tests in Dark Tower.

## Quick Start

### Install Tooling

```bash
# Install cargo-fuzz
cargo install cargo-fuzz

# Install nightly Rust (required for fuzzing)
rustup toolchain install nightly
```

### Run a Fuzz Target

```bash
# Navigate to crate with fuzz targets
cd crates/media-protocol

# List available fuzz targets
cargo fuzz list

# Run a specific target for 60 seconds
cargo fuzz run codec_decode -- -max_total_time=60

# Run with more memory (default: 2GB)
cargo fuzz run codec_decode -- -rss_limit_mb=4096

# Run with multiple workers (parallel fuzzing)
cargo fuzz run codec_decode -- -workers=4
```

### Check Coverage

```bash
# Generate coverage report for fuzz target
cargo fuzz coverage codec_decode

# View coverage HTML report
open fuzz/coverage/codec_decode/index.html
```

## Writing Fuzz Targets

### Template: Basic Fuzzer

```rust
// crates/your-crate/fuzz/fuzz_targets/your_function.rs
#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Your function that should not panic on any input
    let _ = your_crate::your_function(data);
});
```

### Template: Structured Fuzzer

For more complex input structures, use the `arbitrary` crate:

```rust
#![no_main]
use libfuzzer_sys::fuzz_target;
use arbitrary::Arbitrary;

#[derive(Arbitrary, Debug)]
struct FuzzInput {
    field1: u32,
    field2: String,
    field3: Vec<u8>,
}

fuzz_target!(|input: FuzzInput| {
    let _ = your_crate::process(input.field1, &input.field2, &input.field3);
});
```

### Template: Stateful Fuzzer

For testing sequences of operations:

```rust
#![no_main]
use libfuzzer_sys::fuzz_target;
use arbitrary::Arbitrary;

#[derive(Arbitrary, Debug)]
enum Operation {
    Insert { key: String, value: u64 },
    Get { key: String },
    Delete { key: String },
}

fuzz_target!(|ops: Vec<Operation>| {
    let mut map = YourDataStructure::new();

    for op in ops {
        match op {
            Operation::Insert { key, value } => {
                let _ = map.insert(key, value);
            }
            Operation::Get { key } => {
                let _ = map.get(&key);
            }
            Operation::Delete { key } => {
                let _ = map.delete(&key);
            }
        }
    }
});
```

### Template: Round-Trip Fuzzer

Verify encoding/decoding correctness:

```rust
#![no_main]
use libfuzzer_sys::fuzz_target;
use arbitrary::Arbitrary;

#[derive(Arbitrary, Debug, Clone, PartialEq)]
struct YourStruct {
    // Fields...
}

fuzz_target!(|original: YourStruct| {
    // Encode
    let encoded = encode(&original).expect("Encoding valid struct should succeed");

    // Decode
    let decoded = decode(&encoded).expect("Decoding our own encoding should succeed");

    // Verify round-trip
    assert_eq!(decoded, original, "Round-trip must preserve data");
});
```

## Debugging Crashes

### Step 1: Reproduce Crash Locally

When CI reports a crash, the artifact is uploaded. Download it and reproduce:

```bash
# Download artifact from GitHub Actions
# Save to fuzz/artifacts/target_name/crash-abc123

# Reproduce the crash
cargo fuzz run target_name fuzz/artifacts/target_name/crash-abc123

# The fuzzer will crash, showing the panic message
```

### Step 2: Minimize Crashing Input

Reduce the crash to the smallest possible input:

```bash
cargo fuzz tmin target_name fuzz/artifacts/target_name/crash-abc123

# Output: fuzz/artifacts/target_name/minimized-abc123
# This is the smallest input that still triggers the crash
```

### Step 3: Debug with rust-lldb/rust-gdb

```bash
# Build fuzz target in debug mode
cargo +nightly fuzz build target_name

# Run with debugger
rust-lldb target/x86_64-unknown-linux-gnu/debug/target_name \
  fuzz/artifacts/target_name/minimized-abc123

# Inside lldb:
(lldb) run
# Program crashes...

(lldb) bt  # Backtrace
(lldb) frame variable  # Inspect variables
```

### Step 4: Inspect Crash Input

```bash
# View hex dump of crashing input
hexdump -C fuzz/artifacts/target_name/minimized-abc123

# For text-based inputs
cat fuzz/artifacts/target_name/minimized-abc123
```

### Step 5: Write Regression Test

After fixing the bug, add a regression test:

```rust
#[test]
fn test_crash_abc123_regression() {
    // Input that previously caused crash
    let input = &[0x12, 0x34, 0x56, ...];

    // Should now return error instead of panic
    let result = your_function(input);
    assert!(result.is_err());
}
```

### Step 6: Add to Corpus

If the crash revealed an interesting input not in the corpus:

```bash
# Add minimized crash to corpus
cp fuzz/artifacts/target_name/minimized-abc123 \
   fuzz/corpus/target_name/
```

## Corpus Management

### Seed Corpus

Create initial corpus from existing test data:

```bash
# Create corpus directory
mkdir -p fuzz/corpus/target_name

# Add valid inputs from tests
cp tests/fixtures/valid_frame_1.bin fuzz/corpus/target_name/
cp tests/fixtures/valid_frame_2.bin fuzz/corpus/target_name/
```

### Merge Corpora

After running fuzzers independently, merge their discoveries:

```bash
# Machine 1 runs fuzzer, produces corpus1/
# Machine 2 runs fuzzer, produces corpus2/

# Merge both into main corpus
cargo fuzz cmin target_name corpus1/ corpus2/ fuzz/corpus/target_name/

# This keeps only unique inputs (removes redundant ones)
```

### Corpus Minimization

Periodically minimize corpus to keep it manageable:

```bash
# Before: 10,000 inputs
cargo fuzz cmin target_name

# After: ~500 inputs with same coverage
```

### Corpus Storage

For large corpora (>100MB), use Git LFS:

```bash
# Install Git LFS
git lfs install

# Track corpus files
git lfs track "fuzz/corpus/**/*.bin"

# Commit .gitattributes
git add .gitattributes
git commit -m "Track fuzz corpus with Git LFS"
```

## Running with Sanitizers

### AddressSanitizer (Memory Safety)

```bash
# Detects: buffer overflows, use-after-free, double-free
export RUSTFLAGS="-Zsanitizer=address"
cargo +nightly fuzz run target_name -- -max_total_time=300
```

### UndefinedBehaviorSanitizer (UB Detection)

```bash
# Detects: integer overflow, invalid enum values, null pointer deref
export RUSTFLAGS="-Zsanitizer=undefined"
cargo +nightly fuzz run target_name -- -max_total_time=300
```

### MemorySanitizer (Uninitialized Memory)

```bash
# Detects: reads of uninitialized memory
export RUSTFLAGS="-Zsanitizer=memory"
cargo +nightly fuzz run target_name -- -max_total_time=300
```

### Combined Sanitizers

```bash
# Run all sanitizers in sequence
for sanitizer in address undefined memory; do
  echo "Running with $sanitizer sanitizer..."
  export RUSTFLAGS="-Zsanitizer=$sanitizer"
  cargo +nightly fuzz run target_name -- -max_total_time=60 || exit 1
done
```

## Advanced Techniques

### Dictionary-Guided Fuzzing

For parsers with specific keywords or magic bytes:

```bash
# Create dictionary file
cat > fuzz/dictionaries/media_frame.dict <<EOF
# Media frame keywords
"VERSION"
"\x00\x00\x00\x01"  # Version 1 header
"\x2A"              # 42-byte marker
EOF

# Use dictionary
cargo fuzz run codec_decode -- -dict=fuzz/dictionaries/media_frame.dict
```

### Value Profiling

Enable value profiling for better coverage:

```bash
cargo fuzz run target_name -- -use_value_profile=1
```

### Custom Mutator

For domain-specific mutations:

```rust
#![no_main]
use libfuzzer_sys::{fuzz_target, fuzz_mutator};

// Custom mutator for media frames
fuzz_mutator!(|data: &mut [u8], size: usize, max_size: usize, seed: u32| {
    // Your custom mutation logic
    // Example: flip bits only in header (first 42 bytes)
    if size >= 42 {
        let header_byte = (seed as usize % 42);
        data[header_byte] ^= 1 << (seed % 8);
    }
    size
});

fuzz_target!(|data: &[u8]| {
    let _ = decode_media_frame(data);
});
```

### Persistent Mode (Faster)

For in-process fuzzing (10-20x faster):

```rust
#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Avoid allocations that reset between iterations
    static mut BUFFER: [u8; 4096] = [0; 4096];

    unsafe {
        if data.len() <= BUFFER.len() {
            BUFFER[..data.len()].copy_from_slice(data);
            let _ = process(&BUFFER[..data.len()]);
        }
    }
});
```

## CI/CD Integration

### Local Pre-Commit Check

Before pushing, run quick fuzz check:

```bash
#!/bin/bash
# .git/hooks/pre-push

echo "Running fuzz tests (30s each)..."
for target in $(cargo fuzz list); do
  echo "Fuzzing $target..."
  cargo fuzz run $target -- -max_total_time=30 || exit 1
done
```

### GitHub Actions Workflow

```yaml
name: Fuzz Tests

on: [pull_request]

jobs:
  fuzz:
    runs-on: ubuntu-latest
    strategy:
      matrix:
        target:
          - codec_decode
          - jwt_validation
          - signaling_messages
    steps:
      - uses: actions/checkout@v4
      - uses: actions-rust-lang/setup-rust-toolchain@v1
        with:
          toolchain: nightly

      - name: Install cargo-fuzz
        run: cargo install cargo-fuzz

      - name: Run fuzzer
        run: cargo fuzz run ${{ matrix.target }} -- -max_total_time=60

      - name: Check for crashes
        run: |
          if [ -d fuzz/artifacts/${{ matrix.target }} ]; then
            echo "::error::Fuzzer crashed!"
            ls -la fuzz/artifacts/${{ matrix.target }}
            exit 1
          fi

      - name: Upload artifacts
        if: failure()
        uses: actions/upload-artifact@v3
        with:
          name: fuzz-${{ matrix.target }}
          path: fuzz/artifacts/${{ matrix.target }}/
```

## Performance Tips

### Maximize Executions Per Second

**1. Use release builds** (default in cargo-fuzz):
```bash
# Already optimized by default
cargo fuzz run target_name
```

**2. Reduce input size limits**:
```bash
# Limit max input size to 1KB (faster)
cargo fuzz run target_name -- -max_len=1024
```

**3. Use multiple workers**:
```bash
# Use all CPU cores
cargo fuzz run target_name -- -workers=$(nproc)
```

**4. Disable expensive checks in fuzz builds**:
```rust
fuzz_target!(|data: &[u8]| {
    #[cfg(not(fuzzing))]
    expensive_validation();  // Skip in fuzz builds

    let _ = parse(data);
});
```

### Benchmark Fuzzer Performance

```bash
# Run for 60s and measure exec/s
cargo fuzz run target_name -- -max_total_time=60 -print_final_stats=1

# Look for "exec/s" metric
# Target: >10,000 exec/s for simple parsers
```

## Troubleshooting

### Fuzzer Hangs or Runs Slowly

**Problem**: Fuzzer stuck at low exec/s (<100)

**Solutions**:
1. **Profile the target**:
   ```bash
   cargo flamegraph --bin target_name
   ```

2. **Reduce input complexity**:
   ```rust
   // Before: Allows any size
   fuzz_target!(|data: &[u8]| { ... });

   // After: Limit to reasonable size
   fuzz_target!(|data: &[u8]| {
       if data.len() > 1024 { return; }
       ...
   });
   ```

3. **Remove expensive operations**:
   ```rust
   fuzz_target!(|data: &[u8]| {
       let parsed = parse(data)?;

       // Don't validate in fuzz builds (too slow)
       #[cfg(not(fuzzing))]
       validate_expensive(&parsed)?;
   });
   ```

### Out of Memory Errors

**Problem**: Fuzzer crashes with OOM

**Solutions**:
1. **Increase memory limit**:
   ```bash
   cargo fuzz run target -- -rss_limit_mb=4096
   ```

2. **Limit input size**:
   ```bash
   cargo fuzz run target -- -max_len=1024
   ```

3. **Fix memory leaks**:
   ```bash
   # Run with LeakSanitizer
   export RUSTFLAGS="-Zsanitizer=address"
   cargo +nightly fuzz run target
   ```

### No New Coverage

**Problem**: Fuzzer finds no new paths after initial run

**Solutions**:
1. **Use dictionary**:
   ```bash
   cargo fuzz run target -- -dict=fuzz/dict.txt
   ```

2. **Enable value profiling**:
   ```bash
   cargo fuzz run target -- -use_value_profile=1
   ```

3. **Review target logic**:
   - Is there unreachable code?
   - Are there input validation checks blocking fuzzer?

## Fuzz Target Examples

### Example 1: Media Frame Decoder

```rust
// crates/media-protocol/fuzz/fuzz_targets/codec_decode.rs
#![no_main]
use libfuzzer_sys::fuzz_target;
use media_protocol::codec::decode_frame;
use bytes::Bytes;

fuzz_target!(|data: &[u8]| {
    // Should handle any input gracefully
    let mut buf = Bytes::copy_from_slice(data);
    let _ = decode_frame(&mut buf);
});
```

### Example 2: JWT Validation

```rust
// crates/ac-service/fuzz/fuzz_targets/jwt_validation.rs
#![no_main]
use libfuzzer_sys::fuzz_target;
use ac_service::crypto::jwt::validate_token;
use ed25519_dalek::PublicKey;

// Fixed public key for validation
const PUBLIC_KEY: &[u8; 32] = b"test_public_key_32_bytes_long!!!";

fuzz_target!(|data: &[u8]| {
    if let Ok(token) = std::str::from_utf8(data) {
        let pubkey = PublicKey::from_bytes(PUBLIC_KEY).unwrap();
        let _ = validate_token(token, &pubkey);
    }
});
```

### Example 3: Protocol Buffer Deserialization

```rust
// crates/proto-gen/fuzz/fuzz_targets/signaling.rs
#![no_main]
use libfuzzer_sys::fuzz_target;
use prost::Message;
use proto_gen::signaling::ClientMessage;

fuzz_target!(|data: &[u8]| {
    let _ = ClientMessage::decode(data);
});
```

### Example 4: Database Input Fuzzing

```rust
// crates/ac-service/fuzz/fuzz_targets/db_inputs.rs
#![no_main]
use libfuzzer_sys::fuzz_target;
use arbitrary::Arbitrary;

#[derive(Arbitrary, Debug)]
struct FuzzDbInput {
    client_id: String,
    secret: String,
    scopes: Vec<String>,
}

fuzz_target!(|input: FuzzDbInput| {
    // Only validate input format, don't actually insert
    // (too slow for fuzzing)

    if input.client_id.len() > 255 { return; }
    if input.secret.len() > 1024 { return; }
    if input.scopes.len() > 50 { return; }

    // Simulate validation logic
    let _ = validate_client_id(&input.client_id);
    let _ = validate_scopes(&input.scopes);
});
```

## Checklist: Fuzz Target Readiness

Before merging a fuzz target, verify:

- [ ] **Target compiles**: `cargo fuzz build target_name`
- [ ] **Runs without crashes**: `cargo fuzz run target_name -- -runs=10000`
- [ ] **Achieves good coverage**: `cargo fuzz coverage target_name` (>80%)
- [ ] **Has seed corpus**: At least 10 valid inputs in `fuzz/corpus/target_name/`
- [ ] **Fast enough**: >1000 exec/s minimum
- [ ] **Documented**: Comment explaining what it tests
- [ ] **Added to CI**: Listed in `.github/workflows/fuzz.yml`
- [ ] **No panics**: Uses `Result` for errors, never panics

## Resources

- **Rust Fuzz Book**: https://rust-fuzz.github.io/book/
- **cargo-fuzz docs**: https://rust-fuzz.github.io/book/cargo-fuzz.html
- **libFuzzer docs**: https://llvm.org/docs/LibFuzzer.html
- **Arbitrary crate**: https://docs.rs/arbitrary/latest/arbitrary/
- **Dark Tower ADR-0006**: Fuzz Testing Strategy

## Support

**Questions?** Ask in #engineering Slack channel

**Found a crash?** Create GitHub issue with `security` label and attach artifact

**Improving fuzz targets?** Submit PR with updated target + corpus
