# Dark Tower - Claude Code Context

## Project

Rust video conferencing platform using WebTransport/HTTP/3, PostgreSQL, and Protocol Buffers.

| Service | Crate |
|---------|-------|
| Auth Controller | `crates/ac-service/` |
| Global Controller | `crates/gc-service/` |
| Meeting Controller | `crates/mc-service/` |
| Media Handler | `crates/mh-service/` |
| Shared | `crates/common/` |

Architecture: `docs/ARCHITECTURE.md` | Status: `docs/PROJECT_STATUS.md`

## Specialists

| Name | Domain |
|------|--------|
| `auth-controller` | Authentication, JWT, JWKS, federation |
| `global-controller` | HTTP/3 API, meeting management |
| `meeting-controller` | WebTransport signaling, sessions |
| `media-handler` | Media forwarding, quality adaptation |
| `database` | PostgreSQL schema, migrations, queries |
| `protocol` | Protocol Buffers, API contracts, versioning |
| `infrastructure` | Kubernetes, Terraform, IaC |
| `code-reviewer` | Code quality, Rust idioms, ADR compliance |
| `dry-reviewer` | Cross-service duplication detection |
| `test` | E2E tests, coverage, quality gates (MANDATORY cross-cutting) |
| `security` | Threat modeling, cryptography (MANDATORY cross-cutting) |
| `observability` | Metrics, logging, tracing, SLOs (MANDATORY cross-cutting) |
| `operations` | Deployment safety, runbooks, cost (MANDATORY cross-cutting) |

Definitions: `.claude/agents/*.md`

## Documentation

- `docs/ARCHITECTURE.md` - System design, scaling strategy
- `docs/API_CONTRACTS.md` - API specifications
- `docs/decisions/` - Architecture Decision Records (ADRs)
- `docs/debates/` - Multi-agent design debates

## Workflows

- `/devloop "task"` - Implementation via Agent Teams (`.claude/skills/devloop/SKILL.md`)
- `/debate "question"` - Cross-cutting design decisions (`.claude/skills/debate/SKILL.md`)
- `/user-story "story"` - Story decomposition (`.claude/skills/user-story/SKILL.md`)

