# Key ADRs for Code Review

Last updated: 2026-02-10

## Finding Relevant ADRs

**Step 1**: Identify affected components from the changeset

**Step 2**: Check ADRs by category below

**Step 3**: Read the ADR and check MUST/SHOULD/MAY requirements

## Universal ADRs (Check for ALL Changes)

| ADR | Title | Key Requirements |
|-----|-------|------------------|
| ADR-0002 | No Panic Policy | No `.unwrap()`, `.expect()`, `panic!()` in production code |
| ADR-0004 | API Versioning | All endpoints use `/api/v1/...` pattern |

## ADRs by Component

### Auth Controller (`crates/ac-service/`)
| ADR | Title | When to Check |
|-----|-------|---------------|
| ADR-0003 | Service Authentication | Token format, scopes, JWT claims |
| ADR-0007 | Token Lifetime Strategy | Token TTL, refresh patterns |
| ADR-0008 | Key Rotation Strategy | JWKS, key overlap, rotation timing |

### Global Controller (`crates/gc-service/`)
| ADR | Title | When to Check |
|-----|-------|---------------|
| ADR-0010 | GC Architecture | OAuth integration, TokenManager, service clients |
| ADR-0020 | User Auth & Meeting Access | User tokens, meeting access patterns |

### Meeting Controller (`crates/mc-service/`)
| ADR | Title | When to Check |
|-----|-------|---------------|
| ADR-0011 | Observability Framework | Metrics naming, SLO definitions |
| ADR-0023 | MC Architecture | Actors, session management, participant tracking |

### Testing
| ADR | Title | When to Check |
|-----|-------|---------------|
| ADR-0005 | Integration Testing | Test infrastructure, coverage targets |
| ADR-0006 | Fuzz Testing | Fuzzer setup, input validation |
| ADR-0009 | Integration Test Infra | Database testing, test isolation |
| ADR-0014 | Environment Integration Tests | Kind cluster tests, env-tests crate |

### Infrastructure
| ADR | Title | When to Check |
|-----|-------|---------------|
| ADR-0012 | Infrastructure Architecture | K8s manifests, deployment patterns |
| ADR-0013 | Local Development | Docker compose, local setup |

### Process & Workflow
| ADR | Title | When to Check |
|-----|-------|---------------|
| ADR-0015 | Principles Guards | Pre-commit validation |
| ADR-0016 | Development Loop | Dev-loop workflow |
| ADR-0017 | Specialist Knowledge | Knowledge file updates |
| ADR-0018 | Dev-Loop Checkpointing | State recovery |
| ADR-0019 | DRY Reviewer | Cross-service duplication |
| ADR-0022 | Skill-Based Dev-Loop | Skill invocation |

## Violation Severity

| ADR Keyword | Severity | Blocks Merge? |
|-------------|----------|---------------|
| MUST, REQUIRED, SHALL | BLOCKER | Yes |
| SHOULD, RECOMMENDED | CRITICAL | Usually |
| MAY, OPTIONAL | MAJOR | No |

## Quick Reference

All ADRs are in `docs/decisions/adr-NNNN-*.md`

```bash
# List all ADRs
ls docs/decisions/adr-*.md

# Search ADRs for a keyword
grep -l "keyword" docs/decisions/adr-*.md
```
