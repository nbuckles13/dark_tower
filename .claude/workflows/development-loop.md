# Development Loop Workflow

The Development Loop is the primary workflow for implementing features. It combines specialist ownership, context injection, verification, code review, and reflection.

---

## When to Use

| Scenario | Use Loop? | Notes |
|----------|-----------|-------|
| Implement new feature | Yes | Standard flow |
| Bug fix | Yes | Unless trivial one-liner |
| Refactoring | Yes | Tests catch regressions |
| Documentation only | No | No verification needed |
| Exploration/research | No | No code to verify |

When starting, announce:
> *"Starting development loop (specialist-owned verification, max 5 iterations)"*

---

## Loop States

| State | Description | Next State | Step File |
|-------|-------------|------------|-----------|
| `implementation` | Specialist implements + verifies | `validation` | `development-loop/step-implementation.md` |
| `validation` | Orchestrator re-runs verification | `code_review` (pass) or `implementation` (fail) | `development-loop/step-validation.md` |
| `code_review` | 4 specialists review | `reflection` (approved) or `implementation` (findings) | `code-review.md` |
| `reflection` | Update knowledge files | `complete` | `development-loop/step-reflection.md` |
| `complete` | Done | - | - |

---

## Step Reference

| Step | File | When to Read |
|------|------|--------------|
| Implementation | `development-loop/step-implementation.md` | Starting a dev-loop |
| Validation | `development-loop/step-validation.md` | After implementation |
| Code Review | `code-review.md` | After validation passes |
| Reflection | `development-loop/step-reflection.md` | After code review approves |
| Output Format | `development-loop/output-documentation.md` | Creating output files |
| Recovery | `development-loop/session-restore.md` | Resuming interrupted loops |

---

## Orchestrator Checklist

### Before Each State Transition

After completing any step, **immediately** update the Loop State in the output file.

#### 1. After invoking implementing specialist
- Write Loop State with agent ID
- Set Current Step to `implementation`
- Set Iteration to `1`

#### 2. After specialist returns
- Update Current Step to `validation`
- Re-run verification

#### 3. After validation passes
- Update Current Step to `code_review`
- Invoke code reviewers

#### 4. When invoking each code reviewer ⚠️ CRITICAL
- **Immediately** update Loop State with reviewer agent ID
- Do NOT wait until all reviewers are done
- Reviewer IDs are needed for reflection

#### 5. After code review is clean
- Update Current Step to `reflection`
- Resume implementing specialist for reflection
- Resume each reviewer for reflection (using saved agent IDs)

#### 6. After all reflections complete
- **Run Pre-Completion Validation Checklist** (see `output-documentation.md`)
- Verify no TBD/placeholder content in main.md
- Verify all required sections have actual content
- Update Current Step to `complete`
- Update Duration in output file header

### Before Switching Tasks

If user requests a different task while a dev-loop is in progress:
1. Check Loop State in the current output file
2. If Current Step is NOT `complete`:
   - Ask user: "We have an incomplete dev-loop at step '{step}'. Complete it first or pause?"

### Common Mistakes to Avoid

- ❌ Forgetting to save reviewer agent IDs when invoking code review
- ❌ Skipping reflection step after code review is clean
- ❌ Switching to new user request without checking if loop is complete
- ❌ Leaving Loop State at `code_review` after reviewers approve
- ❌ Marking `complete` without validating main.md has actual content (not TBD)
- ❌ Not verifying specialist wrote to the correct output directory

---

## Iteration Handling

When findings require fixes (code review → implementation):

1. Update Loop State iteration counter
2. Specialist writes to checkpoint file
3. Update main.md with iteration details
4. Resume specialist with checkpoint context
5. After fixes: Re-run verification → re-run code review

**Max iterations**: 5. After 5 failed attempts, exit loop and inform user.

---

## Deadlock Handling

If specialists deadlock (e.g., code review disagreements that can't be resolved):
- Exit the loop
- Present the situation to the user
- Let the user decide how to proceed

---

## Loop State Section Format

The orchestrator maintains this section in the output file:

```markdown
## Loop State (Internal)

| Field | Value |
|-------|-------|
| Implementing Agent | `abc123` |
| Current Step | code_review |
| Iteration | 2 |
| Security Reviewer | `def456` |
| Test Reviewer | `ghi789` |
| Code Reviewer | `jkl012` |
| DRY Reviewer | `mno345` |
```

---

## Quick Reference

### Agent IDs to Track

| Agent | When Saved | Used For |
|-------|------------|----------|
| Implementing specialist | Initial invoke | Fix cycles, reflection |
| Security reviewer | Code review | Reflection |
| Test reviewer | Code review | Reflection |
| Code Reviewer | Code review | Reflection |
| DRY Reviewer | Code review | Reflection |

### Verification Commands

```bash
# Dev-loop output validation
./scripts/workflow/verify-dev-loop.sh --output-dir docs/dev-loop-outputs/YYYY-MM-DD-{task}

# Full code verification
./scripts/verify-completion.sh --verbose
```

### Categories Shorthand

| Category | Key Concerns |
|----------|--------------|
| `crypto` | secrets, keys, hashing, encryption |
| `jwt` | token validation, claims, expiry |
| `logging` | no secrets in logs, structured format |
| `queries` | parameterized SQL, no injection |
| `errors` | no panics, proper types |
| `input` | validation, limits, sanitization |
| `testing` | test ownership, three tiers, determinism |
| `concurrency` | actor pattern, message passing |
| `api-design` | URL versioning, deprecation |
| `observability` | privacy-by-default, metrics, spans |

---

## Related Workflows

| Workflow | When to Use | Relationship |
|----------|-------------|--------------|
| `multi-agent-debate.md` | Cross-cutting design | Happens BEFORE loop, produces ADR |
| `code-review.md` | Quality gate | Integrated INTO loop (step 4) |
| `orchestrator-guide.md` | General orchestration | This loop is a key subprocess |
| `process-review-record.md` | Process failures | Use when loop reveals gaps |
