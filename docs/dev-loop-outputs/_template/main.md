# Dev-Loop Output: {Task Title}

**Date**: YYYY-MM-DD
**Task**: Brief description of what was implemented
**Branch**: `branch-name`
**Duration**: ~Xm (approximate total time)

---

## Loop State (Internal)

<!-- This section is maintained by the orchestrator for state recovery after context compression. -->
<!-- Do not edit manually - the orchestrator updates this as the loop progresses. -->

| Field | Value |
|-------|-------|
| Implementing Agent | `{agent_id}` |
| Implementing Specialist | `{specialist-name}` |
| Current Step | `{implementation|validation|code_review|reflection|complete}` |
| Iteration | `{1-5}` |
| Security Reviewer | `{agent_id or pending}` |
| Test Reviewer | `{agent_id or pending}` |
| Code Reviewer | `{agent_id or pending}` |
| DRY Reviewer | `{agent_id or pending}` |

<!-- ORCHESTRATOR REMINDER:
     - Update this table at EVERY state transition (see development-loop.md "Orchestrator Checklist")
     - Capture reviewer agent IDs AS SOON as you invoke each reviewer
     - When step is code_review and all reviewers approve, MUST advance to reflection
     - Only mark complete after ALL reflections are done
     - Before switching to a new user request, check if Current Step != complete
     - Each specialist writes to their own checkpoint file (see _template/specialist.md)
-->

---

## Task Overview

### Objective
{What was the goal of this task?}

### Scope
- **Service(s)**: {Which services were affected}
- **Schema**: {Database schema changes? Yes/No}
- **Cross-cutting**: {Does this affect multiple services? Yes/No}

### Debate Decision
{NEEDED/NOT NEEDED} - {Brief justification}

{If debate was needed, link to debate record: `docs/debates/YYYY-MM-DD-{topic}.md`}

---

## Pre-Work

{Any pending changes committed before starting, dependencies resolved, etc.}

{Or "None" if no pre-work was required}

---

## Implementation Summary

### {Priority/Category 1}
| Item | Before | After |
|------|--------|-------|
| {field/function} | {old} | {new} |

### {Priority/Category 2}
{Description of changes}

### Additional Changes
{Any other notable changes made during implementation}

---

## Files Modified

```
{Output of: git diff --stat HEAD}
```

### Key Changes by File
| File | Changes |
|------|---------|
| `path/to/file.rs` | {Brief description} |

---

## Dev-Loop Verification Steps

### Layer 1: cargo check
**Status**: PASS/FAIL
**Duration**: ~Xs
**Output**: {Any relevant notes}

### Layer 2: cargo fmt
**Status**: PASS/FAIL
**Duration**: ~Xs
**Output**: {Any relevant notes}

### Layer 3: Simple Guards
**Status**: ALL PASS / X FAILED
**Duration**: ~Xs

| Guard | Status |
|-------|--------|
| api-version-check | PASS/FAIL |
| no-hardcoded-secrets | PASS/FAIL |
| no-pii-in-logs | PASS/FAIL |
| no-secrets-in-logs | PASS/FAIL |
| no-test-removal | PASS/FAIL |
| test-coverage | PASS/FAIL |

{Details on any failures}

### Layer 4: Unit Tests
**Status**: PASS/FAIL
**Duration**: ~Xs
**Output**: {Test counts, any failures}

### Layer 5: All Tests (Integration)
**Status**: PASS/FAIL
**Duration**: ~Xs
**Tests**: {X passed, Y failed}

{Details on any failures}

### Layer 6: Clippy
**Status**: PASS/FAIL
**Duration**: ~Xs
**Output**: {Any warnings}

### Layer 7: Semantic Guards
**Status**: PASS/MIXED/FAIL
**Duration**: ~Xs per file

| File | Verdict | Notes |
|------|---------|-------|
| `path/to/file.rs` | SAFE/UNSAFE | {Brief notes} |

{Details on any UNSAFE verdicts - were they valid concerns or false positives?}

---

## Code Review Results

### Security Specialist
**Verdict**: APPROVED / FINDINGS

{Key findings or "No issues found"}

### Test Specialist
**Verdict**: APPROVED / FINDINGS

{Key findings or "No issues found"}

### Code Quality Reviewer
**Verdict**: APPROVED / FINDINGS

{Key findings or "No issues found"}

### DRY Reviewer
**Verdict**: APPROVED / FINDINGS

**Blocking findings** (BLOCKING - code exists in common but wasn't used):
{List any BLOCKINGs that must be fixed, or "None"}

**Tech debt findings** (TECH_DEBT - opportunities for extraction):
{List findings documented below, or "None"}

{Add other reviewers as applicable: Observability, Operations, Infrastructure}

---

## Tech Debt

<!-- Document all TECH_DEBT findings here. These are non-blocking and tracked for follow-up. -->

### Cross-Service Duplication (from DRY Reviewer)

| Pattern | New Location | Existing Location | Follow-up Task |
|---------|--------------|-------------------|----------------|
| {pattern name} | `crates/X/src/file.rs:line` | `crates/Y/src/file.rs:line` | {Extraction task} |

{Or "No cross-service duplication detected" if DRY review found nothing}

### Temporary Code (from Code Reviewer)

| Item | Location | Reason | Follow-up Task |
|------|----------|--------|----------------|
| {endpoint/function} | `path/to/file.rs:line` | {Why it's temporary} | {Remove when X} |

{Or "No temporary code detected" if Code Reviewer found nothing}

---

## Issues Encountered & Resolutions

### Issue 1: {Brief title}
**Problem**: {What went wrong}
**Resolution**: {How it was fixed}

### Issue 2: {Brief title}
**Problem**: {What went wrong}
**Resolution**: {How it was fixed}

{Add more issues as needed, or "None" if no issues}

---

## Lessons Learned

1. {Key takeaway 1}
2. {Key takeaway 2}
3. {Key takeaway 3}

{Add more as applicable}

---

## Appendix: Verification Commands

```bash
# Commands used for verification
./scripts/verify-completion.sh --layer full

# Individual steps
cargo check --workspace
cargo fmt --all --check
./scripts/guards/run-guards.sh
DATABASE_URL=... cargo test --workspace
DATABASE_URL=... cargo clippy --workspace --lib --bins -- -D warnings
./scripts/guards/semantic/credential-leak.sh path/to/file.rs
```
