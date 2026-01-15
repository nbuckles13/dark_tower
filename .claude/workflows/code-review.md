# Code Review Workflow

## Purpose

This workflow orchestrates a comprehensive multi-specialist code review process for Dark Tower. Similar to the multi-agent debate workflow, this process leverages specialized agents to review code changes from different perspectives before merging.

## When to Use

Trigger this workflow:
- **Before committing significant changes** (>500 lines or critical components)
- **For security-sensitive code** (authentication, cryptography, data access)
- **For new features or major refactors**
- **When requested by user** for any changeset
- **As part of CI/CD pipeline** (GitHub PR integration)

## Workflow Overview

```
1. Identify Changes
   ↓
2. Determine Relevant ADRs
   ↓
3. Parallel Specialist Reviews
   ├─→ Code Reviewer (quality, maintainability, Rust idioms)
   ├─→ Security Specialist (vulnerabilities, crypto, auth)
   ├─→ Test Specialist (coverage, edge cases, test quality)
   ├─→ Observability Specialist (logging, metrics, traces)
   └─→ Operations Specialist (if ops-related: deployments, migrations, configs)
   ↓
4. Synthesize Findings
   ↓
5. Generate Action Plan
   ↓
6. Present to User
```

## Reviewer Participation

| Reviewer | Participation | Focus | Blocking Behavior |
|----------|--------------|-------|-------------------|
| Code Reviewer | Every review | Quality, idioms, maintainability | All findings block |
| Security Specialist | Every review | Vulnerabilities, crypto, auth | All findings block |
| Test Specialist | Every review | Coverage, test quality | All findings block |
| Observability Specialist | Every review | Logging, metrics, traces, SLOs | All findings block |
| **DRY Reviewer** | Every review | Cross-service duplication | **Only BLOCKER blocks** |
| Operations Specialist | Ops-related only | Deployments, migrations, configs, runbooks | All findings block |
| Infrastructure Specialist | Infra-related only | K8s, Terraform, Dockerfiles | All findings block |

**Ops-related changes**: Deployment scripts, database migrations, configuration changes, Kubernetes manifests, Terraform, CI/CD pipelines, credential handling.

### DRY Reviewer Blocking Behavior

The DRY Reviewer has **different blocking behavior** from other reviewers (see ADR-0019):

| Severity | Trigger | Blocking? | Action |
|----------|---------|-----------|--------|
| 🔴 BLOCKER | Code EXISTS in `common` but wasn't used | **Yes** | Must fix before approval |
| 🟠 CRITICAL | >90% similar to another service | No | Document as tech debt |
| 🟡 MAJOR | 70-90% similar to another service | No | Document as tech debt |
| 🟢 MINOR | 50-70% similar | No | Document as tech debt |

**Why different?**
- BLOCKER = Code already exists in `common` crate but wasn't imported (a mistake)
- Non-BLOCKER = Code could be extracted to `common` (an opportunity, not a mistake)

Non-BLOCKER findings are documented in the dev-loop output under "Tech Debt: Cross-Service Duplication" and result in follow-up tasks.

## Workflow Steps

### Step 1: Identify Changes

**Orchestrator Action**:
1. Determine what files changed (use `git diff` or list of modified files)
2. Calculate scope:
   - Line count
   - Number of files
   - Components affected (auth, database, API, media, etc.)
3. Classify change type:
   - New feature
   - Bug fix
   - Refactor
   - Security update
   - Performance optimization

**Output**: Change summary document

### Step 2: Determine Relevant ADRs and Principles

**Orchestrator Action**:
1. List all ADRs in `docs/decisions/`
2. Map changes to relevant ADRs based on:
   - File paths (e.g., `crates/ac-service/` → ADR-0003)
   - Keywords (e.g., "JWT", "OAuth" → ADR-0003)
   - Component tags in ADR metadata
3. Create focused ADR list (typically 2-5 ADRs per review)
4. **Match changes to principle categories** (see `contextual-injection.md`):
   - Use same task-to-category mapping as implementation phase
   - Reviewer receives same principles that were given to implementer
   - This ensures consistent standards between implementation and review

**Task-to-Category Mapping** (match file paths/changes against):
```yaml
"password|hash|bcrypt|encrypt|decrypt|key|secret": [crypto, logging]
"query|select|database|migration|sql": [queries, logging]
"jwt|token|auth|oauth|bearer": [crypto, jwt, logging]
"handler|endpoint|route|api": [logging, errors, input]
"client|credential|oauth": [crypto, logging, errors]
"parse|input|validate|request": [input, errors]
```

**Output**: List of relevant ADRs and principle categories

### Step 3: Parallel Specialist Reviews

Run specialists in parallel (multiple Task tool calls in a single message).

**Context Injection for Each Reviewer**:
Each reviewer receives:
1. Their specialist definition (`.claude/agents/{specialist}.md`)
2. Their accumulated knowledge files (`docs/specialist-knowledge/{specialist}/*.md` if they exist)
3. Relevant ADRs and principles
4. The change context and files to review

This ensures reviewers apply both their domain expertise AND learned patterns/gotchas.

#### Review A: Code Reviewer Specialist

**Focus**: Code quality, maintainability, Rust best practices

**Inputs**:
- Specialist definition + accumulated knowledge (`docs/specialist-knowledge/code-reviewer/`)
- List of changed files
- Relevant ADRs (for API design, error handling patterns)
- Change context

**Deliverables**:
- Code quality assessment
- Rust idiom violations
- Maintainability concerns
- Documentation gaps
- Architecture consistency check
- Issues categorized by severity (BLOCKER, CRITICAL, MAJOR, MINOR, SUGGESTION)

#### Review B: Security Specialist

**Focus**: Security vulnerabilities, cryptographic correctness, authentication/authorization

**Inputs**:
- Specialist definition + accumulated knowledge (`docs/specialist-knowledge/security/`)
- List of changed files
- Relevant ADRs (security, authentication)
- Security-focused context

**Deliverables**:
- Security vulnerability assessment
- Cryptographic implementation review
- Authentication/authorization validation
- Input validation check
- Secret management review
- OWASP/CWE mapping
- Issues categorized by severity (CRITICAL, HIGH, MEDIUM, LOW)

#### Review C: Test Specialist

**Focus**: Test coverage, test quality, edge cases

**Inputs**:
- Specialist definition + accumulated knowledge (`docs/specialist-knowledge/test/`)
- List of changed files
- Test files
- Coverage reports (if available)

**Deliverables**:
- Test coverage analysis
- Missing test cases (critical paths, edge cases, error paths)
- Test quality assessment
- Integration test needs
- Performance test recommendations
- Chaos test needs (if applicable)
- Coverage gaps categorized by priority (CRITICAL, HIGH, MEDIUM, LOW)

#### Review D: Observability Specialist

**Focus**: Logging, metrics, tracing, SLO impact

**Inputs**:
- Specialist definition + accumulated knowledge (`docs/specialist-knowledge/observability/`)
- List of changed files
- Relevant ADRs (observability, SLOs)
- Observability-focused context

**Deliverables**:
- Logging assessment (structured logging, correlation IDs)
- Metrics review (naming, cardinality, SLO alignment)
- Trace span coverage (external calls, critical paths)
- SLO impact analysis
- Dashboard recommendations
- Issues categorized by severity (BLOCKER, HIGH, MEDIUM, LOW)

#### Review E: Operations Specialist (if ops-related)

**Trigger**: Include when changes affect deployments, migrations, configs, or operational procedures

**Focus**: Deployment safety, operational readiness, runbook updates

**Inputs**:
- Specialist definition + accumulated knowledge (`docs/specialist-knowledge/operations/`)
- List of changed files
- Migration files
- Configuration changes
- Deployment scripts

**Deliverables**:
- Deployment safety assessment
- Migration rollback plan
- Configuration validation
- Runbook requirements
- Cost implications
- Issues categorized by severity (BLOCKER, HIGH, MEDIUM, LOW)

#### Review F: Infrastructure Specialist (if infra-related)

**Trigger**: Include when changes affect K8s manifests, Terraform, Dockerfiles, or CI/CD

**Focus**: Portability, security boundaries, resource sizing

**Inputs**:
- Specialist definition + accumulated knowledge (`docs/specialist-knowledge/infrastructure/`)
- List of changed files
- Infrastructure files
- Cloud provider configurations

**Deliverables**:
- Portability assessment (cloud-agnostic)
- Security boundary validation
- Resource limit review
- Network policy check
- Issues categorized by severity (BLOCKER, HIGH, MEDIUM, LOW)

#### Review G: DRY Reviewer Specialist

**Focus**: Cross-service code duplication detection

**Inputs**:
- Specialist definition (`.claude/agents/dry-reviewer.md`)
- List of changed files
- Read-only access to ALL service crates

**What to Look For**:
1. **Function signatures**: Similar names or parameter patterns across services
2. **Logic patterns**: Same algorithm implemented differently
3. **Constants**: Duplicated magic numbers, size limits, timeout values
4. **Structs/Types**: Similar data structures that could be shared
5. **Error handling**: Identical error mapping patterns

**Deliverables**:
- Cross-service duplication findings
- Similarity percentage estimates
- Extraction recommendations
- Issues categorized by severity (BLOCKER, CRITICAL, MAJOR, MINOR)

**Blocking Behavior** (DIFFERENT from other reviewers):
- **BLOCKER**: Must fix (code exists in `common` but wasn't used)
- **CRITICAL/MAJOR/MINOR**: Document as tech debt, create follow-up task

**See**: ADR-0019 for full rationale on blocking behavior

### Step 4: Synthesize Findings

**Orchestrator Action**:
1. Collect all specialist reports (4-6 depending on change type)
2. Identify overlapping concerns
3. Prioritize issues by severity:
   - **BLOCKER**: Must fix before merge (security critical, data loss, ADR violation)
   - **CRITICAL**: Should fix before merge (performance, major quality issues)
   - **MAJOR**: Should address soon (code smell, missing tests)
   - **MINOR**: Nice to have (style, optimization)
   - **SUGGESTION**: Future improvements
4. Create consolidated findings document
5. Check for conflicts between specialist recommendations

**Output**: Unified code review report

### Step 5: Generate Action Plan

**Orchestrator Action**:
1. For each BLOCKER/CRITICAL issue, propose specific fix
2. Estimate effort for each fix
3. Identify quick wins vs. substantial refactors
4. Create prioritized task list
5. Determine overall recommendation:
   - ✅ **APPROVE**: Ready to merge
   - ⚠️ **APPROVE WITH MINOR CHANGES**: Can merge after minor fixes
   - 🔄 **REQUEST CHANGES**: Must address blocker/critical issues
   - ❌ **REJECT**: Fundamental issues, needs redesign

**Output**: Action plan with specific next steps

### Step 6: Present to User

**Orchestrator Action**:
1. Present consolidated review report
2. Highlight top 3-5 most important issues
3. Show action plan
4. Present overall recommendation
5. Ask user:
   - Should we address findings now?
   - Which issues to prioritize?
   - Approve action plan?

**Output**: User-friendly review summary

## Specialist Coordination

### How Specialists Communicate

Specialists don't directly communicate. The orchestrator (Claude):
1. Runs all specialists in parallel (single message, multiple Task calls)
2. Receives all reports
3. Synthesizes findings
4. Resolves conflicts or ambiguities
5. Creates unified view

### Handling Conflicting Recommendations

If specialists disagree:
1. **Security > Performance**: Security concerns always win
2. **ADR Compliance > Style**: Architectural standards take precedence
3. **Explicit > Implicit**: Documented decisions override preferences
4. **User Decision**: Present conflict to user for final call

## Review Document Template

```markdown
# Code Review Report: [Component Name]

**Date**: [YYYY-MM-DD]
**Reviewers**: Code Reviewer, Security Specialist, Test Specialist, Observability Specialist [, Operations Specialist, Infrastructure Specialist]
**Scope**: [X files, Y lines changed]
**Change Type**: [Feature/Bugfix/Refactor/etc.]

## Executive Summary
[2-3 sentence overview of changes and overall quality]

## Overall Recommendation
- [ ] ✅ APPROVE
- [ ] ⚠️ APPROVE WITH MINOR CHANGES
- [ ] 🔄 REQUEST CHANGES (current recommendation)
- [ ] ❌ REJECT

## Critical Findings

### 🔴 BLOCKER Issues (Must Fix Before Merge)
1. [Issue description] - **[Specialist]** - `file.rs:123`
   - Impact: [Security/Data Loss/ADR Violation]
   - Fix: [Specific action required]

### 🟠 CRITICAL Issues (Should Fix Before Merge)
[List with file references and proposed fixes]

## By Specialist

### Code Quality Review
[Code Reviewer findings summary]
- Positive highlights
- Issues found
- Recommendations

### Security Review
[Security Specialist findings summary]
- Vulnerabilities identified
- Cryptographic concerns
- Authentication/authorization issues
- Recommendations

### Test Coverage Review
[Test Specialist findings summary]
- Coverage percentage
- Missing test cases
- Test quality issues
- Chaos test coverage (if applicable)
- Recommendations

### Observability Review
[Observability Specialist findings summary]
- Logging coverage
- Metrics coverage
- Trace span coverage
- SLO impact
- Dashboard validation (if applicable)
- Recommendations

#### Dashboard Validation Checklist (if PR includes dashboard changes)
- [ ] Dashboard metric queries match service metrics catalog (`docs/observability/metrics/`)
- [ ] Dashboard job labels match Prometheus scrape targets (e.g., `ac-service-local` for local dev)
- [ ] Dashboard works in both local development and cloud environments
- [ ] No placeholder or generic metric names (see PRR-0001)

### Operations Review (if ops-related)
[Operations Specialist findings summary]
- Deployment safety
- Migration rollback plan
- Runbook requirements
- Cost implications
- Recommendations

### Infrastructure Review (if infra-related)
[Infrastructure Specialist findings summary]
- Portability assessment
- Security boundaries
- Resource sizing
- Recommendations

### DRY Review
[DRY Reviewer findings summary]
- Cross-service duplication detected
- Similarity assessments
- Extraction recommendations

**Blocking findings** (must fix):
- [Any BLOCKER findings - code exists in common but wasn't used]

**Tech debt findings** (documented, fix later):
- [CRITICAL/MAJOR/MINOR findings - opportunities for extraction]

## Tech Debt: Cross-Service Duplication

| Pattern | New Location | Existing Location | Severity | Follow-up Task |
|---------|--------------|-------------------|----------|----------------|
| [pattern] | `crates/X/src/file.rs:line` | `crates/Y/src/file.rs:line` | CRITICAL | Extract to common |

## ADR Compliance
[Relevant ADRs and compliance status]
- ✅ ADR-XXXX: Compliant
- ⚠️ ADR-YYYY: Partial compliance (details...)
- ❌ ADR-ZZZZ: Non-compliant (must address)

## Principle Compliance
[Matched principle categories and compliance status]

### Principle Checklist
- [ ] **crypto.md**: No hardcoded secrets, EdDSA for signing, bcrypt≥12, CSPRNG
- [ ] **jwt.md**: Token validation, size limits, algorithm enforcement
- [ ] **logging.md**: No secrets in logs, SecretString usage, structured format
- [ ] **queries.md**: Parameterized SQL, no string concatenation
- [ ] **errors.md**: No panics, Result types, generic API messages
- [ ] **input.md**: Length limits, type validation, early rejection

(Check only categories that were matched to this change)

## Action Plan

### Immediate Actions (Before Merge)
1. [Action] - Est: [time] - Priority: BLOCKER
2. [Action] - Est: [time] - Priority: CRITICAL

### Follow-up Actions (Next Sprint)
1. [Action] - Est: [time] - Priority: MAJOR
2. [Action] - Est: [time] - Priority: MINOR

### Future Improvements
- [Suggestion 1]
- [Suggestion 2]

## Metrics
- Files reviewed: X
- Lines reviewed: Y
- Issues found: Z
  - Blocker: A
  - Critical: B
  - Major: C
  - Minor: D
- Estimated fix time: [hours]

## Next Steps
[Specific instructions for addressing findings]
```

## Integration with Development Workflow

### Pre-commit Review (Manual)
```bash
# User requests review
User: "Review the auth controller changes before committing"

# Orchestrator runs workflow
Claude: [Identifies changes, determines ADRs, runs 3 specialists in parallel]
Claude: [Synthesizes findings, creates action plan]
Claude: [Presents report and recommendations]

# User decides
User: "Let's fix the blocker issues first"
Claude: [Addresses findings]
Claude: [Re-runs review if needed]
Claude: [Commits when approved]
```

### GitHub PR Review (Future)

**Architecture**: Custom GitHub Action using Dark Tower specialists

The GitHub Action will:
1. Clone repository including `.claude/` directory
2. Read specialist definitions from `.claude/agents/*.md`
3. Read workflow from `.claude/workflows/code-review.md`
4. Call Claude API with our custom specialist prompts
5. Execute the same review process as local workflow
6. Post consolidated review as PR comment
7. Post individual findings as inline code comments
8. Set PR status (approve/request changes/block merge)

**Benefits**:
- ✅ Uses project-specific specialists with ADR knowledge
- ✅ Consistent review criteria (local and CI)
- ✅ Specialists evolve with project
- ✅ Domain expertise (Auth Controller, Security, etc.)
- ✅ Full control over review process

**Implementation** (Phase 2 - Future):
```yaml
# .github/workflows/code-review.yml
name: Dark Tower Code Review
on: pull_request

jobs:
  review:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
        with:
          fetch-depth: 0  # Full history for git diff

      - name: Dark Tower Code Review
        uses: ./.github/actions/dark-tower-review
        with:
          anthropic_api_key: ${{ secrets.ANTHROPIC_API_KEY }}
          specialists_path: .claude/agents
          workflow_path: .claude/workflows/code-review.md
          post_comments: true
          block_on_blocker: true
        env:
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
```

**Custom Action** (`.github/actions/dark-tower-review/action.yml`):
- Written in TypeScript or Python
- Reads specialist definitions dynamically
- Constructs Claude API prompts
- Runs parallel specialist reviews
- Synthesizes findings
- Posts to GitHub PR

**Why Custom Action Over Generic**:
- Generic reviewers don't know Dark Tower's ADRs, OAuth flows, debate decisions
- Our specialists encode institutional knowledge
- Project-specific security requirements (EdDSA, AES-256-GCM, bcrypt cost)
- Consistent with local development workflow

## Quality Gates

### Merge Criteria

**Required**:
- ✅ No BLOCKER issues
- ✅ All CRITICAL security issues addressed
- ✅ ADR compliance (no violations)
- ✅ Builds successfully
- ✅ All existing tests pass

**Recommended**:
- ⚠️ No CRITICAL quality issues
- ⚠️ Test coverage > 80% for new code
- ⚠️ No major performance regressions

**Nice to Have**:
- 💡 MAJOR issues addressed
- 💡 Test coverage > 90%
- 💡 Documentation complete

## Success Metrics

Track over time:
- **Review Turnaround Time**: < 30 minutes for typical PR
- **Defect Detection Rate**: > 90% of bugs caught pre-merge
- **False Positive Rate**: < 10% of issues invalid
- **Rework Rate**: < 20% of PRs require major changes
- **Security Vulnerability Escape Rate**: 0% critical vulnerabilities to production

## Tips for Effective Reviews

**For Orchestrator**:
- Be thorough but pragmatic
- Focus specialist attention on high-risk areas
- Don't let perfect be the enemy of good
- Security is non-negotiable
- Provide actionable, specific feedback

**For Specialists**:
- Be constructive, not critical
- Explain *why*, not just *what*
- Suggest solutions, not just problems
- Acknowledge good code
- Reference standards (ADRs, RFCs, best practices)

**For Users**:
- Don't skip reviews for "quick fixes"
- Address BLOCKER issues before requesting re-review
- Ask questions if recommendations unclear
- Push back on low-value feedback
- Treat reviews as learning opportunities

## Continuous Improvement

After each review:
1. Collect feedback on review quality
2. Track recurring issues (opportunities for tooling/linting)
3. Update specialist prompts based on learnings
4. Refine severity criteria if needed
5. Adjust process for efficiency

---

**Remember**: The goal is **high-quality, secure code delivered rapidly**. Reviews should enable confidence, not create bottlenecks. Be thorough on security, pragmatic on style.

## Related Workflows

- **contextual-injection.md**: How to match tasks to principle categories
- **orchestrator-guide.md**: Principles are also injected during debates
- **multi-agent-debate.md**: Debate mechanics and format
