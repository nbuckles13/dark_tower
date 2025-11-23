# Dark Tower - Build Requirements

This document explains the native build dependencies required for Dark Tower and why they're needed.

## Overview

Dark Tower is primarily written in Rust, but several dependencies require **native compilation** (C/C++ code) during the build process. This means you need a C compiler toolchain installed on your system.

**Quick Install** (Ubuntu/WSL2):
```bash
sudo apt update && sudo apt install -y build-essential pkg-config protobuf-compiler
```

See [Platform-Specific Instructions](#platform-specific-installation) for other operating systems.

---

## Why Native Dependencies?

### Cryptography Performance

Dark Tower uses **ring** for cryptographic operations (TLS, JWT signing, password hashing). The `ring` library includes highly-optimized implementations of cryptographic algorithms written in C and assembly:

- **EdDSA (Ed25519)**: Used for JWT signing in Auth Controller
- **AES-256-GCM**: Used for encrypting signing keys at rest
- **TLS/QUIC**: Used for WebTransport and HTTPS connections

These C/assembly implementations are **10-100x faster** than pure Rust equivalents, which is critical for:
- Low-latency WebTransport connections (Meeting Controller, Media Handler)
- High-throughput token validation (Auth Controller validates thousands of JWTs per second)
- Password hashing with bcrypt (Auth Controller user authentication)

### Protocol Buffers

Dark Tower uses Protocol Buffers for service-to-service communication. The `proto-gen` crate requires the `protoc` compiler to generate Rust code from `.proto` files at build time.

---

## Required Build Tools

### 1. C/C++ Compiler

**Purpose**: Compile native code in dependencies like `ring`, `bcrypt`

**Minimum Version**:
- GCC: 7.0+
- Clang: 7.0+
- MSVC: Visual Studio 2019+

**Required By**:
- `ring` v0.17.14 (primary crypto library)
- `ring` v0.16.20 (via wtransport dependency)
- `bcrypt` v0.15.1 (password hashing)
- Many transitive dependencies (libc, getrandom, parking_lot_core, etc.)

### 2. Protocol Buffer Compiler (`protoc`)

**Purpose**: Generate Rust code from `.proto` files

**Minimum Version**: 3.0+

**Required By**:
- `proto-gen` crate (builds protobuf definitions from `proto/` directory)

**Note**: This is **REQUIRED**, not optional. The project will not build without it.

### 3. pkg-config

**Purpose**: Build configuration and library detection

**Required By**:
- Some dependencies use `pkg-config` to locate system libraries during build

**Note**: Not strictly required with current dependency tree (we use rustls instead of OpenSSL), but recommended for compatibility.

---

## Platform-Specific Installation

### Ubuntu/Debian (including WSL2)

```bash
sudo apt update
sudo apt install -y build-essential pkg-config protobuf-compiler
```

**What gets installed**:
- `gcc` - GNU C compiler
- `g++` - GNU C++ compiler
- `make` - Build automation
- `libc6-dev` - C standard library headers
- `dpkg-dev` - Package development tools
- `pkg-config` - Build configuration tool
- `protoc` - Protocol Buffer compiler

**Verify installation**:
```bash
gcc --version        # Should show 7.0+
protoc --version     # Should show 3.0+
pkg-config --version
```

### macOS

```bash
# Install Xcode Command Line Tools (includes clang/gcc)
xcode-select --install

# Install protobuf and pkg-config
brew install protobuf pkg-config
```

**What gets installed**:
- `clang` - LLVM C/C++ compiler (symlinked as `gcc`)
- `make` - Build automation
- `protoc` - Protocol Buffer compiler
- `pkg-config` - Build configuration tool

**Verify installation**:
```bash
gcc --version        # Actually runs clang
protoc --version     # Should show 3.0+
```

### Fedora/RHEL/CentOS

```bash
# Fedora/RHEL 8+/CentOS Stream
sudo dnf groupinstall "Development Tools"
sudo dnf install pkg-config protobuf-compiler

# RHEL 7/CentOS 7 (older)
sudo yum groupinstall "Development Tools"
sudo yum install pkg-config protobuf-compiler
```

### Arch Linux

```bash
sudo pacman -S base-devel protobuf pkg-config
```

### Windows

#### Option 1: WSL2 (Recommended)

```bash
# Install Ubuntu on WSL2, then follow Ubuntu instructions
sudo apt update && sudo apt install -y build-essential pkg-config protobuf-compiler
```

**Advantages**:
- Native Linux build environment
- Faster compilation than native Windows
- Consistent with CI/CD pipelines

#### Option 2: Native Windows (Visual Studio)

1. **Install Visual Studio 2019 or later**:
   - Download from [visualstudio.microsoft.com](https://visualstudio.microsoft.com/)
   - During installation, select "Desktop development with C++"
   - This installs MSVC compiler and Windows 10 SDK

2. **Install Protocol Buffer compiler**:
   ```powershell
   # Using Chocolatey
   choco install protoc

   # Or download manually from:
   # https://github.com/protocolbuffers/protobuf/releases
   # Extract and add to PATH
   ```

3. **Verify installation**:
   ```powershell
   cl.exe     # MSVC compiler (should show version)
   protoc --version
   ```

#### Option 3: MSYS2/MinGW

```bash
# Install MSYS2 from https://www.msys2.org/
# Then in MSYS2 terminal:
pacman -S mingw-w64-x86_64-gcc mingw-w64-x86_64-pkg-config mingw-w64-x86_64-protobuf
```

---

## Dependency Details

### Primary Native Dependencies

#### ring v0.17.14

**What it does**: Cryptographic operations (TLS, signatures, encryption)

**Native code**:
- C implementations of AES, ChaCha20, Poly1305
- Assembly optimizations for x86_64, ARM
- BoringSSL-derived code

**Build requirements**:
- C compiler (gcc/clang/MSVC)
- No external libraries (self-contained)

**Performance impact**: 10-100x faster than pure Rust equivalents

**Used by**:
- `rustls` → TLS for Auth Controller HTTPS
- `quinn` → QUIC for WebTransport (Meeting/Media Controllers)
- `jsonwebtoken` → JWT signing (Auth Controller)
- `webpki` → Certificate verification

#### bcrypt v0.15.1

**What it does**: Password hashing with bcrypt algorithm

**Native code**: Blowfish cipher implementation in C

**Build requirements**: C compiler

**Used by**: Auth Controller (`crates/ac-service`) for user authentication

#### prost-build v0.13.5

**What it does**: Generates Rust code from `.proto` files at build time

**Build requirements**:
- System `protoc` binary in PATH
- Pure Rust otherwise (no C compiler needed for prost itself)

**Used by**: `crates/proto-gen/build.rs`

**Generated code location**: `crates/proto-gen/src/generated/`

### Secondary Native Dependencies

Many crates have build scripts that require C compiler:
- `libc` - OS interface
- `getrandom` - System RNG access
- `parking_lot_core` - Parking lot primitives
- `proc-macro2` - Procedural macro support
- `num-traits` - Numeric traits
- `serde` - Serialization framework

These are **transitive dependencies** - you don't directly use them, but they're required by crates you do use.

---

## Common Build Errors

### Error: "linker `cc` not found"

**Full error**:
```
error: linker `cc` not found
  |
  = note: No such file or directory (os error 2)

error: could not compile `ring` due to 1 previous error
```

**Cause**: Missing C compiler

**Solution**:
```bash
# Ubuntu/Debian/WSL2
sudo apt install build-essential

# macOS
xcode-select --install

# Fedora/RHEL
sudo dnf groupinstall "Development Tools"
```

**Why this happens**: The `ring` crate's build script invokes the system C compiler (`cc` or `gcc`) to compile its C/assembly code. If no compiler is found, the build fails immediately.

---

### Error: "protoc failed: No such file or directory"

**Full error**:
```
error: failed to run custom build command for `proto-gen v0.1.0`

Caused by:
  process didn't exit successfully: `/home/nathan/code/dark_tower/target/debug/build/proto-gen-xxx/build-script-build`
--- stderr
Error: Custom { kind: NotFound, error: "protoc failed: No such file or directory (os error 2)" }
```

**Cause**: Missing Protocol Buffer compiler

**Solution**:
```bash
# Ubuntu/Debian
sudo apt install protobuf-compiler

# macOS
brew install protobuf

# Verify
protoc --version  # Should show 3.0+
```

**Why this happens**: The `proto-gen` crate's build script (`crates/proto-gen/build.rs`) calls `protoc` to generate Rust code from `proto/*.proto` files. If `protoc` is not installed or not in PATH, the build fails.

---

### Error: "pkg-config not found"

**Full error**:
```
error: failed to run custom build command for `openssl-sys`
--- stderr
thread 'main' panicked at 'Unable to locate pkg-config'
```

**Note**: This error is **NOT currently present** in Dark Tower (we use rustls instead of OpenSSL), but may occur if dependencies change in the future.

**Solution**:
```bash
# Ubuntu/Debian
sudo apt install pkg-config

# macOS
brew install pkg-config
```

---

### Error: Build is very slow on WSL2

**Symptoms**: `cargo build` takes 10+ minutes

**Cause**: Project located on Windows filesystem (`/mnt/c/...`) instead of WSL2 filesystem

**Solution**:
```bash
# Ensure project is in WSL2 home directory
cd /home/nathan/code/dark_tower  # ✓ Correct

# NOT on Windows mount
# /mnt/c/Users/nathan/code/dark_tower  # ✗ Slow
```

**Current project location**: `/home/nathan/code/dark_tower` ✓ Correct

**Optional optimizations**:
```bash
# Use sccache for faster incremental builds
cargo install sccache
export RUSTC_WRAPPER=sccache

# Use mold linker (Linux only, much faster than ld)
sudo apt install mold
export RUSTFLAGS="-C link-arg=-fuse-ld=mold"
```

---

## Minimum Versions

| Tool | Minimum Version | Recommended | Notes |
|------|----------------|-------------|-------|
| Rust | 1.75.0 | 1.91+ | Project requirement |
| GCC | 7.0 | 11+ | For ring compilation |
| Clang | 7.0 | 14+ | macOS default |
| MSVC | VS 2019 | VS 2022 | Windows only |
| protoc | 3.0 | 3.21+ | Protocol Buffers |
| pkg-config | Any | Latest | Build config |
| Make | Any | 4.0+ | Build automation |

---

## Verifying Your Setup

After installing build tools, verify everything is working:

```bash
# Check Rust
rustc --version
cargo --version

# Check C compiler
gcc --version    # or clang --version on macOS
cc --version     # Should work on all platforms

# Check protoc
protoc --version

# Check pkg-config
pkg-config --version

# Test build
cd /path/to/dark_tower
cargo build --package proto-gen  # Tests protoc
cargo build --package ac-service  # Tests gcc + protoc
```

**Expected output**:
```
   Compiling ring v0.17.14
   Compiling bcrypt v0.15.1
   Compiling proto-gen v0.1.0
   Compiling ac-service v0.1.0
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 2m 34s
```

If you see compilation errors, check the [Common Build Errors](#common-build-errors) section.

---

## CI/CD Considerations

### GitHub Actions

```yaml
# .github/workflows/rust.yml
steps:
  - name: Install build dependencies (Ubuntu)
    run: |
      sudo apt update
      sudo apt install -y build-essential pkg-config protobuf-compiler

  - name: Build
    run: cargo build --verbose
```

### Docker Builds

```dockerfile
# Dockerfile
FROM rust:1.91 as builder

# Install build dependencies
RUN apt-get update && apt-get install -y \
    build-essential \
    pkg-config \
    protobuf-compiler \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY . .
RUN cargo build --release
```

---

## Performance Implications

### Build Time

With native dependencies, expect longer build times on first build:

| Project Size | First Build | Incremental |
|-------------|-------------|-------------|
| Small (AC only) | 2-5 min | 10-30 sec |
| Full workspace | 5-15 min | 30-60 sec |

**Optimizations**:
- Use `sccache` for caching compiled dependencies
- Use `mold` linker on Linux (5-10x faster linking)
- Use `cargo build --release` for production (slower build, faster runtime)

### Runtime Performance

Native dependencies provide significant runtime performance improvements:

| Operation | Pure Rust | With ring | Speedup |
|-----------|-----------|-----------|---------|
| EdDSA signature | ~500 μs | ~50 μs | 10x |
| AES-256-GCM | ~1 ms | ~10 μs | 100x |
| bcrypt hash | ~300 ms | ~300 ms | 1x* |

*bcrypt is intentionally slow (password hashing)

**For Dark Tower**:
- Auth Controller can validate 20,000 JWTs/sec (with ring's EdDSA)
- WebTransport can handle 10,000+ concurrent connections (with ring's QUIC crypto)

---

## Troubleshooting Resources

- **Rust installation**: https://rustup.rs/
- **ring documentation**: https://docs.rs/ring/
- **Protocol Buffers**: https://protobuf.dev/
- **Dark Tower development guide**: [DEVELOPMENT.md](DEVELOPMENT.md)
- **Build issues**: [GitHub Issues](https://github.com/nbuckles13/dark_tower/issues)

---

## Summary

**Critical build requirements**:
1. ✅ C compiler (gcc/clang/MSVC)
2. ✅ Protocol Buffer compiler (protoc)
3. ⚠️ pkg-config (recommended)

**One-liner install (Ubuntu/WSL2)**:
```bash
sudo apt update && sudo apt install -y build-essential pkg-config protobuf-compiler
```

**Why required**: Cryptography performance (ring), Protocol Buffers code generation (proto-gen)

**Impact if missing**: Build will fail with "linker `cc` not found" or "protoc failed" errors

See [DEVELOPMENT.md](DEVELOPMENT.md) for complete development environment setup.
