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

## Layout

- `crates/` — Rust services + shared `common/` (Cargo workspace); see table above
- `packages/` — TypeScript/Svelte client monorepo: `sdk-core`, `sdk-svelte`, `web-app`, `test-utils` (pnpm + Nx)
- `proto/` — Protocol Buffers contracts (source of truth for wire types)
- `migrations/` — PostgreSQL migrations
- `infra/` — devloop container tooling (`infra/devloop/`), Kind cluster setup (`infra/kind/`)
- `scripts/` — polyglot validation pipeline: `layer-all.sh` → `layer1..7.sh` (ADR-0033)

## Working Conventions

Apply to every agent here — host orchestrator, devloop teammates, and specialist subagents.

- **Fix, don't defer.** Make trivial in-tree edits now — never park a one-line fix in `docs/TODO.md`. Separate the source edit (do it now) from any host-side rebuild/deploy step (call that out as the remaining action). Only defer genuinely task-sized work, and say why it's task-sized.
- **Fail loudly; never mask.** Environment and code problems must fail loudly. Do not silently skip, ignore, or work around a failure to make progress — a masked failure becomes a silent regression later (skipped tests, bypassed gates, stale environments).
- **Single source of truth.** When two places encode the same value, they will drift. Derive one from the other, or add a guard that fails validation on drift. Confirm something isn't already the SSoT before duplicating it.
- **Config over hardcoding.** Prefer config-driven values to hardcoded constants.
- **Infra topology stays out of application code.** Rust reads config; it does not parse K8s hostnames or compute ports from pod ordinals.

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
| `client` | Browser SDK, Svelte web app, media pipeline |
| `dry-reviewer` | Cross-service duplication detection |
| `semantic-guard` | Semantic anti-patterns pattern guards can't catch (credential-leak, actor-blocking, error-context, metrics-path) |
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
