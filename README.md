# Dark Tower

A modern, high-performance video conferencing platform built with Rust and WebTransport.

## Overview

Dark Tower is an ambitious open-source video conferencing platform designed for scalability, performance, and user flexibility. Built entirely with AI-generated code, it leverages cutting-edge web technologies including WebTransport, WebCodec, and QUIC to deliver low-latency, high-quality video conferencing experiences.

## Architecture

The platform consists of four main components:

1. **Global Controller** - Global API entry point with DNS-based geographic routing
2. **Meeting Controller** - Manages individual meetings, signaling, and participant coordination
3. **Media Handler** - Handles media routing, transcoding, and mixing (SFU architecture)
4. **Client** - Web-based user interface built with Svelte and WebCodec

## Project Structure

```
dark_tower/
├── crates/                    # Rust workspace
│   ├── global-controller/     # Global Controller service
│   ├── meeting-controller/    # Meeting Controller service
│   ├── media-handler/         # Media Handler service
│   ├── common/                # Shared utilities and types
│   ├── proto-gen/             # Generated Protocol Buffer code
│   └── media-protocol/        # Proprietary media protocol implementation
├── client/                    # Svelte web client
├── proto/                     # Protocol Buffer definitions
├── infra/                     # Infrastructure as Code
│   ├── terraform/             # Terraform configurations
│   ├── kubernetes/            # Kubernetes manifests
│   └── docker/                # Dockerfiles
├── scripts/                   # Build and development scripts
├── tests/                     # Integration and E2E tests
│   ├── integration/           # Integration tests
│   └── e2e/                   # End-to-end tests
└── docs/                      # Documentation

```

## Technology Stack

- **Backend**: Rust with Tokio async runtime
- **Frontend**: Svelte with TypeScript
- **Transport**: WebTransport (QUIC), HTTP/3
- **Media**: WebCodec API, custom media protocol
- **Databases**: PostgreSQL (persistent), Redis (ephemeral)
- **Orchestration**: Kubernetes
- **Observability**: OpenTelemetry, Prometheus, Grafana

See [TECHNICAL_STACK.md](docs/TECHNICAL_STACK.md) for complete details.

## Key Features

- End-to-end encrypted media streams
- Sub-250ms join-to-media latency
- Support for multiple simultaneous content shares
- Multiple cameras per participant
- Highly scalable architecture (10,000+ concurrent participants per region)
- Power user controls and customization
- Open APIs for extensibility

## Development

### Prerequisites

- **Rust** 1.75+ (install via [rustup](https://rustup.rs/))
- **C Compiler** and build tools (gcc/clang/MSVC)
- **Protocol Buffer Compiler** (`protoc` 3.0+)
- **Node.js** 20+
- **Docker** and Docker Compose
- **PostgreSQL** 15+
- **Redis** 7+

**⚠️ Important**: Dark Tower requires a C compiler for native dependencies (`ring`, `rustls`, `bcrypt`). See [DEVELOPMENT.md](docs/DEVELOPMENT.md#system-build-tools) for platform-specific setup.

#### Quick Setup (Ubuntu/WSL2)

```bash
# Install build dependencies
sudo apt update && sudo apt install -y build-essential pkg-config protobuf-compiler

# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Verify installation
rustc --version && gcc --version && protoc --version
```

#### Quick Setup (macOS)

```bash
# Install Xcode Command Line Tools
xcode-select --install

# Install protobuf
brew install protobuf pkg-config

# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

See [DEVELOPMENT.md](docs/DEVELOPMENT.md) for detailed setup instructions for other platforms.

### Getting Started

```bash
# Clone the repository
git clone https://github.com/nbuckles13/dark_tower.git
cd dark_tower

# Build all components
cargo build

# Run tests
cargo test

# Start local development environment
docker-compose up -d

# Run the client
cd client
npm install
npm run dev
```

## Testing

We maintain ambitious test coverage goals:

- **Unit Tests**: 90%+ code coverage
- **Integration Tests**: All critical paths
- **E2E Tests**: All user flows
- **Performance Tests**: Baseline and regression testing

## Code Quality

All code must:
- Pass `clippy` with pedantic warnings enabled
- Be formatted with `rustfmt`
- Have zero compile errors and zero warnings
- Pass AI-powered code review
- Follow security best practices

## Contributing

This project is built entirely with AI-generated code. All contributions should follow the established patterns and quality standards.

## License

MIT OR Apache-2.0

## Project Status

🚧 **Phase 1: Foundation & Architecture** - In Progress

See the project roadmap in [docs/](docs/) for more details.
