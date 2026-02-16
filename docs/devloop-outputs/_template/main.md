# Devloop Output: {Task Title}

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
     - Use /devloop-status to check state
     - If interrupted, restart the devloop; main.md records start commit for rollback
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

## Devloop Verification Steps

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
**Verdict**: CLEAR / RESOLVED / ESCALATED
**Findings**: {count} found, {count} fixed, {count} deferred

{Key findings and resolutions, or "No findings"}

### Test Specialist
**Verdict**: CLEAR / RESOLVED / ESCALATED
**Findings**: {count} found, {count} fixed, {count} deferred

{Key findings and resolutions, or "No findings"}

### Observability Specialist
**Verdict**: CLEAR / RESOLVED / ESCALATED
**Findings**: {count} found, {count} fixed, {count} deferred

{Key findings and resolutions, or "No findings"}

### Code Quality Reviewer
**Verdict**: CLEAR / RESOLVED / ESCALATED
**Findings**: {count} found, {count} fixed, {count} deferred

{Key findings and resolutions, or "No findings"}

### DRY Reviewer
**Verdict**: CLEAR / RESOLVED / ESCALATED

**True duplication findings** (entered fix-or-defer flow):
{List findings sent to implementer, or "None"}

**Extraction opportunities** (tech debt observations):
{List opportunities documented below, or "None"}

### Operations Reviewer
**Verdict**: CLEAR / RESOLVED / ESCALATED
**Findings**: {count} found, {count} fixed, {count} deferred

{Key findings and resolutions, or "No findings"}

---

## Tech Debt

<!-- Document all accepted deferrals and DRY extraction opportunities here. -->

### Deferred Findings

| Finding | Reviewer | Location | Deferral Justification | Follow-up Task |
|---------|----------|----------|------------------------|----------------|
| {description} | {reviewer} | `file.rs:line` | {implementer's justification for deferral} | {task ref} |

{Or "No deferred findings" if all findings were fixed}

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

If this devloop needs to be reverted:
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
