# Dark Tower - Technical Stack

## Overview
This document outlines the technology choices for the Dark Tower video conferencing platform.

## Backend Components

### Language & Runtime
- **Primary Language**: Rust
  - Rationale: Performance, memory safety, excellent async support, strong type system
  - Target: Stable Rust (latest stable release)

### Frameworks & Libraries
- **Async Runtime**: Tokio
- **HTTP Server**: Axum or Actix-web
- **WebTransport**: Quinn (QUIC implementation)
- **Database Clients**:
  - tokio-postgres (PostgreSQL)
  - redis-rs (Redis)

## Frontend Client

### Framework
- **Primary Framework**: Svelte
  - Rationale: Minimal runtime overhead, excellent performance, great reactivity
  - Build Tool: Vite

### Core Technologies
- **Language**: TypeScript
- **Media APIs**:
  - WebCodec API for encoding/decoding
  - WebTransport API for QUIC-based media transport
  - WebRTC API (as needed for fallback/compatibility)

## Communication Protocols

### Transport Layer
- **HTTP/1.1, HTTP/2, HTTP/3**: One-shot and transactional requests
  - Authentication
  - Meeting creation/management
  - Configuration queries
  - HTTP/3 over QUIC for improved performance
- **WebTransport (QUIC)**: Real-time bidirectional communication
  - Signaling
  - Media transport
  - State synchronization
  - Participant updates

### Message Format
- **Protocol Buffers**: Signaling and control messages
  - Binary efficiency
  - Schema validation
  - Excellent Rust support via prost
- **Proprietary Media Protocol over QUIC**: Real-time media streams
  - Custom binary format optimized for video/audio frames
  - Minimal overhead for maximum throughput
  - Frame metadata and synchronization
  - To be defined based on WebCodec requirements
- **JSON**: Secondary format for debugging and admin interfaces only

## Data Storage

### Persistent Storage
- **PostgreSQL**: Relational data
  - User accounts and profiles
  - Meeting metadata and history
  - Room configurations
  - Recordings metadata
  - Audit logs

### Ephemeral Storage
- **Redis**: In-memory data structures
  - Active meeting state
  - Participant session data
  - Real-time presence
  - Rate limiting
  - Caching layer

### Distributed Coordination
- **etcd** or **Consul**: Service discovery and coordination
  - Meeting controller registration
  - Media handler discovery
  - Configuration distribution
  - Leader election (if needed)

## Deployment & Infrastructure

### Containerization
- **Docker**: Container runtime
- **Kubernetes**: Orchestration platform
  - Multi-datacenter deployment
  - Auto-scaling
  - Service mesh integration
  - Rolling updates

### Infrastructure as Code
- **Terraform** or **Pulumi**: Infrastructure provisioning
  - Multi-cloud support
  - Declarative configuration
  - State management

### Service Mesh
- **Istio** or **Linkerd**: Inter-service communication
  - Traffic management
  - Security (mTLS)
  - Observability
  - Load balancing

## Observability

### Metrics & Monitoring
- **OpenTelemetry**: Instrumentation standard
- **Prometheus**: Metrics collection and storage
- **Grafana**: Visualization and dashboards

### Logging
- **Structured Logging**: JSON-formatted logs
- **Log Aggregation**: ELK stack or Loki

### Tracing
- **OpenTelemetry**: Distributed tracing
- **Jaeger** or **Tempo**: Trace backend

## Security

### Authentication & Authorization
- **JWT**: JSON Web Tokens with short TTLs
- **OAuth2/OIDC**: Integration with identity providers
- **TLS 1.3**: All external communication encrypted

### End-to-End Encryption
- **Media Encryption**: End-to-end encrypted media streams
  - Client-generated encryption keys
  - Perfect forward secrecy
  - Server cannot decrypt media content
- **Signaling Encryption**: TLS 1.3 for control plane
- **Key Management**: Client-side key generation and exchange
- **Identity Verification**: Optional participant verification

### Secrets Management
- **HashiCorp Vault** or **Kubernetes Secrets**: Secure credential storage

## CI/CD

### Build & Test
- **GitHub Actions** or **GitLab CI**: Pipeline automation
- **Cargo**: Rust build system
- **Vite**: Frontend build

### Testing
- **Rust**: cargo test, criterion for benchmarks
- **Frontend**: Vitest, Playwright for e2e
- **Integration**: Custom test harnesses
- **Coverage Goals**:
  - Unit tests: 90%+ code coverage
  - Integration tests: All critical paths covered
  - E2E tests: All user flows validated
  - Performance tests: Baseline and regression testing
  - Chaos testing: Failure scenario validation

## Development Environment

### Local Development
- **Docker Compose**: Local multi-service orchestration
- **Hot Reload**: cargo-watch for Rust, Vite for frontend

### Code Quality
- **Rust**:
  - clippy (linting) - must pass with `-W clippy::pedantic`
  - rustfmt (formatting) - enforced
  - **Zero tolerance**: No compile errors, no warnings at pedantic level
- **TypeScript**:
  - ESLint with strict rules
  - Prettier for formatting
  - **Strict mode**: Zero errors, zero warnings
- **AI Code Reviews**:
  - Automated AI-powered code review on all PRs
  - Security vulnerability detection
  - Performance optimization suggestions
  - Best practices enforcement
  - Architecture consistency checks
- **Git Hooks**: pre-commit hooks for formatting and lint checks
- **CI Enforcement**: All PRs must pass full lint and build checks

## Component Architecture

### Global Controller
- Rust/Axum for HTTP endpoints
- PostgreSQL for global state
- Redis for caching
- DNS-based geographic routing

### Meeting Controller
- Rust/Quinn for WebTransport
- Redis for meeting state
- Protocol Buffers for signaling
- Horizontal scaling per region

### Media Handler
- Rust for performance-critical media processing
- WebCodec integration via WebTransport
- Proprietary media protocol for frame transport
- SFU (Selective Forwarding Unit) architecture
- Capability for transcoding and mixing

### Client
- Svelte/TypeScript SPA
- WebTransport for primary transport
- WebCodec for media encoding/decoding
- Progressive enhancement strategy

## Version Control

### Repository Structure
- **Monorepo** approach using Cargo workspace
- Separate crates for each component
- Shared libraries for common functionality

## Browser Support

### Target Browsers
- Chrome/Edge 97+ (WebTransport support)
- Firefox (when WebTransport ships)
- Safari (when WebTransport ships)
- WebRTC fallback for older browsers (Phase 2)

## Performance Targets

### Latency
- **User join to media reception**: < 250ms end-to-end
- Signaling: < 50ms p99
- Media forwarding: < 100ms glass-to-glass latency
- API responses: < 100ms p95

### Scalability
- 10,000+ concurrent participants per region
- 100+ participants per meeting
- 1000+ concurrent meetings per meeting controller

### Availability
- 99.9% uptime target
- Multi-region redundancy
- Graceful degradation
