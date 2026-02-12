# Dev-Loop Output: {Task Title}

**Date**: YYYY-MM-DD
**Task**: Brief description of what was implemented
**Specialist**: {specialist-name}
**Mode**: Agent Teams (v2)
**Branch**: `branch-name`
**Duration**: ~Xm (approximate total time)

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `{git rev-parse HEAD at setup}` |
| Branch | `{current branch}` |

---

## Loop State (Internal)

<!-- This section is maintained by the Lead for state recovery after interruption. -->
<!-- Do not edit manually - the Lead updates this as the loop progresses. -->

| Field | Value |
|-------|-------|
| Phase | `{setup|planning|implementation|review|reflection|complete}` |
| Implementer | `{agent_id or pending}` |
| Implementing Specialist | `{specialist-name}` |
| Iteration | `{1-5}` |
| Security | `{agent_id or pending}` |
| Test | `{agent_id or pending}` |
| Observability | `{agent_id or pending}` |
| Code Quality | `{agent_id or pending}` |
| DRY | `{agent_id or pending}` |
| Operations | `{agent_id or pending}` |

<!-- LEAD REMINDER:
     - Update this table at EVERY phase transition
     - Capture teammate IDs AS SOON as you spawn them
     - When phase is review and all reviewers approve, advance to reflection
     - Only mark complete after ALL reflections are done
     - Each specialist writes to their own checkpoint file
     - Use /dev-loop-status to check state, /dev-loop-restore to recover from interruption
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

## Planning

TBD

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

### Observability Specialist
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

### Operations Reviewer
**Verdict**: APPROVED / FINDINGS

{Key findings or "No issues found"}

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

## Rollback Procedure

If this dev-loop needs to be reverted:
1. Verify start commit from Loop Metadata: `{start_commit}`
2. Review all changes: `git diff {start_commit}..HEAD`
3. Soft reset (preserves changes): `git reset --soft {start_commit}`
4. Hard reset (clean revert): `git reset --hard {start_commit}`
5. For schema changes: rollback requires a forward migration — `git reset` alone is insufficient if migrations were applied
6. For infrastructure changes: may require `skaffold delete` or `kubectl delete -f` if manifests were applied

---

## Reflection

TBD

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
