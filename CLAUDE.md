# Dark Tower - Claude Code Context

**Auto-loaded project context for Claude Code sessions**

---

## ⚠️ ORCHESTRATOR PRE-FLIGHT CHECKLIST

**STOP! Before starting ANY task, answer these questions:**

| Question | If YES → Action |
|----------|-----------------|
| Is this implementation work (code, tests, features)? | → **USE SPECIALISTS** - Do NOT implement directly |
| Does it touch security or crypto? | → **INCLUDE Security specialist** |
| Does it involve writing tests? | → Domain specialist writes, **Test specialist reviews** |
| Does it cross 2+ components? | → **INITIATE A DEBATE** (get user approval first) |
| Does it change database schema? | → **INCLUDE Database specialist** |

**You CAN do directly**: Update Cargo.toml, create docs, orchestrate workflows, synthesize results

**You CANNOT do directly**: Write features, write tests, implement security, change schemas

**If in doubt**: Ask the user or check `.claude/workflows/specialist-decision-matrix.md`

---

## What is Dark Tower?

Dark Tower is a modern, AI-generated video conferencing platform built with Rust and WebTransport. The project uses a multi-service architecture with specialist-led development and multi-agent debates for cross-cutting design decisions.

**Core Technology Stack**:
- **Backend**: Rust (async/await with Tokio)
- **Transport**: WebTransport (QUIC), HTTP/3
- **Database**: PostgreSQL (persistent), Redis (ephemeral)
- **Protocols**: Protocol Buffers (signaling), proprietary binary (media)
- **Security**: OAuth 2.0, EdDSA (Ed25519), AES-256-GCM, bcrypt
- **Frontend** (planned): Svelte, WebCodec

## Architecture Overview

The platform consists of **five main components**:

### 1. Authentication Controller (ac-service) ✅ IMPLEMENTED
- **Status**: Production-ready, 83% test coverage (targeting 95%)
- **Purpose**: Service-to-service authentication via OAuth 2.0 Client Credentials
- **Features**:
  - JWT token issuance (EdDSA signatures)
  - JWKS endpoint for federation
  - Rate limiting (token bucket algorithm)
  - Bcrypt password hashing (cost factor 12)
  - AES-256-GCM encryption at rest for private keys
- **Location**: `crates/ac-service/`
- **Database**: PostgreSQL with sqlx migrations

### 2. Global Controller 🚧 SKELETON
- **Status**: Planned for Phase 5
- **Purpose**: HTTP/3 API gateway, geographic routing, meeting management
- **Location**: `crates/global-controller/`

### 3. Meeting Controller 🚧 SKELETON
- **Status**: Planned for Phase 6
- **Purpose**: WebTransport signaling, session management, participant coordination
- **Location**: `crates/meeting-controller/`

### 4. Media Handler 🚧 SKELETON
- **Status**: Planned for Phase 7
- **Purpose**: SFU (Selective Forwarding Unit) media routing, quality adaptation
- **Location**: `crates/media-handler/`

### 5. Client 📋 PLANNED
- **Status**: Planned for Phase 8
- **Purpose**: Svelte web UI, WebCodec media processing, E2E encryption
- **Location**: `client/`

## Development Model

### Specialist-Led Development

**CRITICAL**: You (Claude Code orchestrator) do NOT implement features directly. Your role is to:
1. Identify which specialists should handle each task
2. Invoke specialist agents to do the work
3. Coordinate debates for cross-cutting features
4. Synthesize results and present to user

**See**: `.claude/DEVELOPMENT_WORKFLOW.md` for complete rules

### Available Specialists

**Service Specialists** (domain experts):
- `auth-controller` - Authentication, JWT, JWKS, federation
- `global-controller` - HTTP/3 API, meeting management
- `meeting-controller` - WebTransport signaling, sessions
- `media-handler` - Media forwarding, quality adaptation

**Domain Specialists** (specific domains):
- `database` - PostgreSQL schema, migrations, queries
- `protocol` - Protocol Buffers, API contracts, versioning
- `infrastructure` - Kubernetes, Terraform, IaC, cloud-agnostic platform
- `code-reviewer` - Code quality, Rust idioms, ADR compliance
- `dry-reviewer` - Cross-service duplication detection (see ADR-0019)

**Cross-Cutting Specialists** (MANDATORY in ALL dev-loops AND debates — see ADR-0024):
- `test` - E2E tests, coverage, chaos testing, quality gates
- `security` - Security architecture, threat modeling, cryptography
- `observability` - Metrics, logging, tracing, SLOs, error budgets
- `operations` - Deployment safety, runbooks, incident response, cost

**Specialist Definitions**: `.claude/agents/*.md`

### When to Use Specialists

**NEVER implement directly**:
- Database schemas or migrations → Database specialist
- Authentication/authorization → Auth Controller specialist
- API endpoints → Service specialist
- Test suites → Test specialist
- Security features → Security specialist

**You CAN do directly**:
- Update Cargo.toml dependencies
- Create documentation files
- Orchestrate workflows
- Synthesize debate results

**If in doubt**: Use the decision matrix in `.claude/workflows/specialist-decision-matrix.md`

### Multi-Agent Debates

**When to initiate debates**:
- Features touching 2+ services
- Protocol or contract changes
- Database schema changes
- Performance/scalability impacts
- Core pattern modifications

**Minimum debate size**: 5 agents (1 domain + Test + Security + Observability + Operations)

**Process**:
1. Propose debate to user (ALWAYS get approval first)
2. Run specialists in rounds (max 10 rounds)
3. Track satisfaction scores (target: 90%+ consensus)
4. Create ADR when consensus reached
5. Implement via specialists

**See**: `.claude/skills/debate/SKILL.md`

## Critical Conventions

### 1. No Panics in Production Code
- **ADR**: `docs/decisions/adr-0002-no-panic-policy.md`
- Use `Result<T, E>` for all fallible operations
- Validate inputs at system boundaries
- `panic!` only allowed in: test code, unreachable invariants, development tools

### 2. Test Coverage Requirements
- **Target**: 95% for security-critical code (auth, crypto, validation)
- **Minimum**: 90% for critical paths
- **P0 tests**: Must pass (security-critical)
- **P1 tests**: Important functionality
- **Fuzz tests**: All parsers and protocols

### 3. Database Queries
- **ALWAYS** use sqlx with compile-time query checking
- **NEVER** use string concatenation for SQL
- All queries are parameterized (SQL injection safe by design)
- Migrations: `migrations/*.sql` (numbered sequentially)

### 4. Security Standards
- **Cryptography**: EdDSA (Ed25519) for signatures, AES-256-GCM for encryption
- **Password hashing**: bcrypt with cost factor 12
- **Token lifetimes**: 1 hour default, configurable
- **Rate limiting**: Required on all authentication endpoints
- **Zero-trust architecture**: Every service-to-service call authenticated

### 5. Error Handling
- Custom error types per crate (e.g., `AcError` in ac-service)
- Implement `std::error::Error` trait
- Use `thiserror` for error definitions
- Map errors at API boundaries (don't leak internal details)

## Build & Test Commands

### Development
```bash
# Build all services
cargo build --workspace

# Run all tests
cargo test --workspace

# Run tests with coverage
cargo llvm-cov --workspace --lcov --output-path lcov.info

# Format code
cargo fmt --all

# Linting
cargo clippy --workspace --lib --bins -- -D warnings
```

### Database
```bash
# Start PostgreSQL (test environment)
docker-compose -f docker-compose.test.yml up -d

# Run migrations
export DATABASE_URL=postgresql://postgres:postgres@localhost:5432/dark_tower_test
sqlx migrate run

# Create new migration
sqlx migrate add <name>
```

### Testing Specific Components
```bash
# Auth Controller tests only
cd crates/ac-service
cargo test

# Integration tests with database
export DATABASE_URL=postgresql://postgres:postgres@localhost:5432/dark_tower_test
cargo test --workspace --verbose

# Fuzz testing
cd crates/ac-service
cargo fuzz run token_validation_fuzzer
```

## Project Structure

```
dark_tower/
├── crates/
│   ├── ac-service/          # Authentication Controller (IMPLEMENTED)
│   ├── ac-test-utils/       # Auth testing utilities (IMPLEMENTED)
│   ├── common/              # Shared types and utilities
│   ├── proto-gen/           # Generated Protocol Buffer code
│   ├── media-protocol/      # Proprietary media protocol
│   ├── global-controller/   # Global API gateway (SKELETON)
│   ├── meeting-controller/  # Meeting signaling (SKELETON)
│   └── media-handler/       # Media routing SFU (SKELETON)
├── client/                  # Svelte web application (PLANNED)
├── proto/                   # Protocol Buffer definitions
│   ├── signaling.proto      # Client ↔ Meeting Controller messages
│   └── internal.proto       # Internal service messages
├── migrations/              # Database migrations (AC schema implemented)
├── docs/
│   ├── debates/             # Multi-agent design debates
│   ├── decisions/           # Architecture Decision Records (ADRs)
│   ├── ARCHITECTURE.md      # System architecture
│   ├── API_CONTRACTS.md     # API specifications
│   ├── DATABASE_SCHEMA.md   # Database design
│   └── PROJECT_STATUS.md    # Current progress tracking
├── .claude/
│   ├── agents/              # Specialist agent definitions
│   ├── workflows/           # Debate and orchestration workflows
│   └── DEVELOPMENT_WORKFLOW.md  # Orchestrator rules
└── infra/
    ├── docker/              # Docker configurations
    └── kubernetes/          # K8s manifests (future)
```

## Documentation Map

### Architecture & Design
- **ARCHITECTURE.md** - System design, component interactions, scaling strategy
- **API_CONTRACTS.md** - API specifications, error handling patterns
- **DATABASE_SCHEMA.md** - PostgreSQL schema, indexes, migrations
- **WEBTRANSPORT_FLOW.md** - WebTransport connection flow, message framing

### Decision History
- **docs/debates/** - Multi-agent design debates (YYYY-MM-DD-{topic}.md)
- **docs/decisions/** - Architecture Decision Records (adr-NNNN-{feature}.md)

### Development Process
- **.claude/DEVELOPMENT_WORKFLOW.md** - Orchestrator rules, specialist usage
- **.claude/skills/dev-loop/SKILL.md** - Dev-loop Agent Teams workflow
- **.claude/skills/debate/SKILL.md** - Multi-agent debate workflow
- **docs/dev-loop-outputs/** - Implementation output tracking
- **FUZZING.md** - Fuzzing strategy and setup

### Current Status
- **PROJECT_STATUS.md** - Current phase, completed work, roadmap
- **.claude/TODO.md** - Technical debt and future work

## Common Development Tasks

### Implementing a Feature or Refactor
1. Run `/dev-loop "task description"` to start the Agent Teams workflow
2. Lead spawns teammates, they handle planning → implementation → review → reflection
3. Track progress in `docs/dev-loop-outputs/YYYY-MM-DD-{task}/main.md`

### Adding a Database Table
1. Create migration: `sqlx migrate add create_table_name`
2. Write SQL in `migrations/NNNN_create_table_name.sql`
3. Update `docs/DATABASE_SCHEMA.md`
4. Invoke Database specialist to review
5. Test migration: `sqlx migrate run`

### Adding an API Endpoint
1. Identify service (GC, MC, MH, AC)
2. Invoke service specialist + Test + Security (minimum)
3. If cross-cutting: Initiate debate (get user approval first)
4. Create ADR if consensus reached
5. Update `docs/API_CONTRACTS.md`

### Security Changes
1. **ALWAYS** invoke Security specialist
2. Include Test specialist for security tests
3. Update threat model if needed
4. Run comprehensive security tests (P0 + P1)
5. Update security documentation

### Implementing Tests
1. **NEVER** implement tests directly
2. Invoke Test specialist to create tests
3. Test specialist determines:
   - What to test (critical paths, edge cases)
   - How to test (unit, integration, fuzz)
   - Coverage requirements
4. Security specialist reviews security test coverage

## Common Pitfalls

### ❌ DON'T
- Implement tests yourself → Invoke Test specialist
- Implement security features yourself → Invoke Security specialist
- Skip migrations when changing schema
- Use `unwrap()` or `expect()` in production code
- Concatenate strings for SQL queries
- Start debates without user approval
- Skip Test or Security specialists in debates
- Forget to create ADR after reaching consensus

### ✅ DO
- Use sqlx compile-time query checking
- Use CSPRNG for security-critical randomness (ring::rand::SystemRandom)
- Add rate limiting to authentication endpoints
- Include Test and Security specialists in ALL debates
- Propose debates to user before starting
- Create ADRs for all consensus-based decisions
- Update PROJECT_STATUS.md when phase changes
- Follow the specialist decision matrix when unsure

## Development Loop Workflow

Single-command workflow with autonomous teammates:

```
/dev-loop "task description" --specialist={name}
```

**How it works**:
- Lead spawns 7 teammates (1 implementer + 6 reviewers)
- Teammates communicate directly with each other
- Lead only intervenes at gates (plan approval, validation, final approval)
- Minimal context accumulation in Lead; teammates preserve full context

**Team composition**: Implementer + Security + Test + Observability + Code Quality + DRY + Operations

### Utility Skills

- `/dev-loop-status` - Check current state of any dev-loop

### Key Aspects

- **Context injection**: Specialist knowledge automatically included
- **Validation pipeline**: check → fmt → guards → tests → clippy → audit + coverage (reported) + artifact-specific layers (see ADR-0024)
- **Git state tracking**: Start commit recorded for rollback (see ADR-0024)
- **Recovery**: Restart from beginning if interrupted; main.md records start commit for rollback

**Code review blocking behavior** (severities: BLOCKER > MAJOR > MINOR; unresolved → TECH_DEBT):
- Security, Observability, Infrastructure: MINOR+ blocks (all findings); non-fixed → TECH_DEBT
- Test, Code Quality, Operations: MAJOR+ blocks; MINOR → TECH_DEBT
- DRY Reviewer: BLOCKER only; MAJOR/MINOR → TECH_DEBT (per ADR-0019)

**Key Files**:
- `.claude/skills/dev-loop/SKILL.md` - Agent Teams workflow
- `.claude/agent-teams/` - Specialist and protocol templates
- `docs/dev-loop-outputs/` - Output files tracking each implementation

**When to use**: Any implementation task (features, tests, refactors, security changes)

---

## Debate Workflow

For design decisions affecting multiple services, use debates to reach consensus:

```
/debate "design question"
```

**How it works**:
- Lead spawns domain + cross-cutting specialists as teammates
- Specialists discuss and update satisfaction scores (0-100)
- Consensus at 90%+ all participants
- Creates ADR when consensus reached

**When to use**: Protocol changes, schema changes, cross-service features, architectural decisions

**Output**: Architecture Decision Record (ADR) only - implementation is separate `/dev-loop`

**Key Files**:
- `.claude/skills/debate/SKILL.md` - Debate workflow
- `.claude/agent-teams/protocols/debate.md` - Debate communication protocol
- `docs/debates/` - Debate records

---

## Specialist Knowledge Files

Each specialist accumulates learnings in dynamic knowledge files:

```
docs/specialist-knowledge/{specialist}/
├── patterns.md      # What works well (successful approaches)
├── gotchas.md       # What to avoid (pitfalls encountered)
└── integration.md   # How to work with other components
```

**Current specialists with knowledge**: auth-controller, global-controller, meeting-controller, security, test, code-reviewer, dry-reviewer, operations, infrastructure, database, protocol, observability, media-handler

**How it works**:
- Knowledge files are injected into specialist prompts at invocation
- After each implementation, reflection captures new learnings
- Knowledge compounds over time, improving specialist effectiveness

**See**: ADR-0017 for architecture details

---

## Current Phase: Phase 4 - Security Hardening

**Focus**: Achieve 95% test coverage, implement P1 security improvements

**Recent Achievements (January 2026)**:
- ✅ SecretBox/SecretString refactor (sensitive data protection)
- ✅ Guard pipeline Phase 1 (credential leak detection)
- ✅ Development loop workflow (specialist-owned verification)
- ✅ Specialist knowledge architecture (dynamic knowledge files)

**Previous Achievements**:
- ✅ Authentication Controller fully implemented
- ✅ P0/P1 security test framework
- ✅ JWT validation security tests (iat, header injection, size limits)
- ✅ SQL injection prevention tests (7 tests)
- ✅ Key rotation implementation (ADR-0008, ADR-0009)
- ✅ Fuzzing infrastructure
- ✅ CI/CD with GitHub Actions

**In Progress**:
- env-tests enhancements (NetworkPolicy, TLS, rate limit validation)
- Code coverage improvement (83% → 95% target)
- Performance benchmarks for auth under attack

**See**: `docs/PROJECT_STATUS.md` for detailed roadmap

## Quick Reference

**Need to know what to do?** Check these in order:
1. This file (CLAUDE.md) - Project context ← You are here
2. `.claude/DEVELOPMENT_WORKFLOW.md` - Orchestrator rules
3. `.claude/workflows/specialist-decision-matrix.md` - When to use specialists
4. `docs/PROJECT_STATUS.md` - Current phase and status
5. `.claude/TODO.md` - Immediate work items

**Starting a new session?**
1. Read `.claude/DEVELOPMENT_WORKFLOW.md`
2. Check `docs/PROJECT_STATUS.md` for current phase
3. Review `.claude/TODO.md` for ongoing work
4. Identify which specialists you'll need today

**Making a commit?**
1. Check `.claude/workflows/pre-commit-checklist.md`
2. Verify workflow compliance (debates, specialists, ADRs)
3. Update documentation if needed
4. Run tests and linting

---

**Remember**: You are the **orchestrator**, not the implementer. Direct the specialists, facilitate debates, and synthesize results. Trust the specialists to do their domain work.
