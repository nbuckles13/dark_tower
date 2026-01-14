# Development Loop Workflow

## Overview

The Development Loop is the primary workflow for implementing features. It combines:
- **Specialist ownership** - Implementing specialist runs verification and fixes issues
- **Context injection** - Principles, patterns, and specialist knowledge injected into prompts
- **Trust-but-verify** - Orchestrator re-runs verification as safety net
- **Code review** - Specialist reviewers validate quality before completion
- **Reflection via resume** - All specialists resumed to capture learnings with intact context
- **Output as proof-of-work** - Specialist writes output file, orchestrator validates
- **Collaboration escalation** - Human involvement after 5 failed attempts

## When to Use

| Scenario | Use Loop? | Notes |
|----------|-----------|-------|
| Implement new feature | Yes | Standard flow |
| Bug fix | Yes | Unless trivial one-liner |
| Refactoring | Yes | Tests catch regressions |
| Documentation only | No | No verification needed |
| Exploration/research | No | No code to verify |

## Loop Flow

```
┌─────────────────────────────────────────────────────────────────┐
│         Development Loop (Specialist-Owned Verification)         │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  1. DEBATE (if cross-cutting)                                   │
│     └─→ Produces ADR with design decisions                      │
│     └─→ See: multi-agent-debate.md                              │
│                                                                  │
│  2. SPECIALIST IMPLEMENTATION (single resumed session)          │
│     └─→ Context injection: principles + knowledge + ADR + task  │
│     └─→ Specialist implements feature                           │
│     └─→ Specialist runs verification (7 layers)                 │
│     └─→ Specialist fixes failures, iterates until pass          │
│     └─→ Specialist writes output file (impl + verification)     │
│     └─→ Returns to orchestrator                                 │
│                                                                  │
│  3. ORCHESTRATOR VALIDATES                                      │
│     └─→ Re-runs verification (trust but verify)                 │
│     └─→ Validates output file has required sections             │
│     └─→ If FAIL: Resume specialist → fix issues                 │
│     └─→ If PASS: Continue to code review                        │
│                                                                  │
│  4. CODE REVIEW (per code-review.md)                            │
│     └─→ Run specialist reviewers in parallel                    │
│     └─→ Save agent IDs for later resume                         │
│     └─→ If ANY findings: Resume impl specialist → fix → step 3  │
│     └─→ If CLEAN: Continue to reflection                        │
│                                                                  │
│  5. REFLECTION (all specialists resumed)                        │
│     └─→ Resume implementing specialist → reflect → update KB    │
│     └─→ Resume each reviewer specialist → reflect → update KB   │
│     └─→ Each appends reflection to output file                  │
│                                                                  │
│  6. FINALIZE OUTPUT                                             │
│     └─→ Orchestrator appends code review results to output file │
│     └─→ Validates all required sections present                 │
│     └─→ Present to user for review                              │
│                                                                  │
│  COLLABORATION (if iteration > 5)                               │
│     └─→ Stop loop, present status to human                      │
│     └─→ Work together to resolve                                │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

## Key Principles

### Specialist Ownership
The implementing specialist owns verification - it runs checks, fixes failures, and iterates until passing. This keeps the specialist's context intact and ensures it learns from its own mistakes.

### Resume for Continuity
All specialists are **resumed** (not re-invoked fresh) for:
- Fix cycles (implementing specialist)
- Reflection (all specialists)

This preserves context so reflection captures genuine learnings, not summaries.

### Output as Proof-of-Work
The specialist writes the dev-loop output file as it works. The orchestrator validates required sections exist - if verification results are missing, the specialist skipped that step.

## Triggering the Loop

**Implicit trigger**: The orchestrator automatically uses this loop for implementation tasks.

When starting, announce:
> *"Starting development loop (specialist-owned verification, max 5 iterations)"*

Report each iteration result as you go.

---

# Part 1: Context Injection

## Injection Order

When building specialist prompts, inject context in this order:

1. **Specialist definition** - From `.claude/agents/{specialist}.md`
2. **Matched principles** - From `docs/principles/` based on task keywords
3. **Specialist knowledge** - From `.claude/agents/{specialist}/` (if exists)
4. **Design context** - ADR summary if from debate
5. **Task context** - The actual task description and existing patterns

## Specialist Knowledge Files

Each specialist may have accumulated knowledge in `.claude/agents/{specialist}/`:
- `patterns.md` - Established approaches for common tasks
- `gotchas.md` - Mistakes to avoid, learned from experience
- `integration.md` - Notes on working with other services

**If these files exist**: Inject their contents after principles, before task context.

**If these files don't exist**: Skip this step (specialist will bootstrap them during first reflection).

## Principle Categories

Self-contained principle files in `docs/principles/`:

| Category | File | Key Concerns |
|----------|------|--------------|
| Cryptography | `crypto.md` | EdDSA, bcrypt, CSPRNG, key rotation, no hardcoded secrets |
| JWT | `jwt.md` | Token validation, claims, expiry, size limits, algorithm attacks |
| Logging | `logging.md` | No PII, no secrets, SecretString, structured format |
| Queries | `queries.md` | Parameterized SQL, org_id filter, no dynamic SQL |
| Errors | `errors.md` | No panics, Result types, generic API messages |
| Input | `input.md` | Length limits, type validation, early rejection |
| Testing | `testing.md` | Test ownership, three tiers, determinism, coverage targets |
| Concurrency | `concurrency.md` | Actor pattern, message passing, no shared mutable state |
| API Design | `api-design.md` | URL versioning, deprecation, protobuf evolution |
| Observability | `observability.md` | Privacy-by-default, metrics naming, spans, SLOs |

## Task-to-Category Mapping

Match task description against patterns to determine which categories to inject:

```yaml
task_patterns:
  "password|hash|bcrypt|encrypt|decrypt|key|secret": [crypto, logging]
  "query|select|database|migration|sql": [queries, logging]
  "jwt|token|auth|oauth|bearer": [crypto, jwt, logging]
  "handler|endpoint|route|api": [logging, errors, input, api-design]
  "client|credential|oauth": [crypto, logging, errors]
  "parse|input|validate|request": [input, errors]
  "test|coverage|fuzz|integration|e2e": [testing, errors]
  "actor|channel|spawn|concurrent|async": [concurrency, errors]
  "version|deprecate|breaking|protobuf": [api-design, errors]
  "metric|trace|span|instrument|log": [observability, logging]
```

**Matching Rules**:
1. Case-insensitive regex match against task description
2. Multiple patterns can match → union of categories
3. Limit to 3-4 categories max per invocation (attention budget)
4. Always include `errors` for any production code

---

# Part 2: Specialist Prompts

## Initial Invocation Template

```markdown
{Specialist definition from .claude/agents/{specialist}.md}

## IMPLEMENTATION MODE

You are being invoked to IMPLEMENT. The design phase is complete - either the task
is straightforward or a debate has already produced an ADR.

**Expected behavior**:
- Explore the codebase as needed to understand existing patterns
- Implement the changes directly
- Run verification and fix issues
- Write the output file

**If requirements are unclear or contradictory**:
- Do NOT guess or make assumptions about ambiguous requirements
- Return to the orchestrator with specific questions about what's unclear
- The orchestrator will either clarify or escalate to a debate/planning phase

The goal is to avoid planning work that's already been designed, while still
allowing you to raise genuine blockers.

## Project Principles (MUST FOLLOW)

{Inject matched category files here}

## Your Accumulated Knowledge

{Contents of .claude/agents/{specialist}/*.md if they exist, or "No accumulated knowledge yet - you'll be asked to reflect after completing this task."}

## Design Context

{ADR summary if from debate, or "N/A" if no debate}

## Existing Patterns

{Relevant code snippets from similar implementations}

## Task

{The actual task description}

## Your Responsibilities

You own the full implementation cycle:

1. **Implement** the feature with tests
2. **Run verification** (all 7 layers):
   - cargo check (must compile)
   - cargo fmt (auto-formats code)
   - ./scripts/guards/run-guards.sh (simple guards)
   - cargo test (all tests must pass)
   - cargo clippy -- -D warnings (no warnings)
   - ./scripts/guards/semantic/credential-leak.sh {changed files} (semantic guards)
3. **Fix failures** and re-run until all pass
4. **Write output file** at `docs/dev-loop-outputs/YYYY-MM-DD-{task-slug}.md`:
   - Use template from `docs/dev-loop-outputs/_template.md`
   - Fill in: Task Overview, Implementation Summary, Files Modified
   - Fill in: Verification Results (all 7 layers with status, timing, any issues)
   - Fill in: Issues Encountered & Resolutions
5. **Return** when verification passes

The orchestrator will re-run verification to confirm, then run code review.
If code review has findings, you'll be resumed to fix them.
After code review is clean, you'll be resumed for reflection.

## Deliverables

1. Implementation code
2. Unit tests for new/modified code
3. All verification checks passing
4. Output file with implementation and verification sections completed
```

## Resume for Fixes Template

When resuming a specialist to fix code review findings:

```markdown
## Code Review Findings (Iteration {N} of 5)

Verification passed ✓

Code review found the following issues:

### 🔴 Security Specialist
1. **{Finding title}** - `{file:line}`
   - Impact: {description}
   - Fix: {suggested fix}

### 🟡 Test Specialist
2. **{Finding title}** - `{file:line}`
   - {description}

{... more findings ...}

Please address ALL findings, then:
1. Re-run verification
2. Update the output file with what you fixed
3. Return when complete
```

## Resume for Reflection Template

When resuming a specialist for reflection (after code review is clean):

```markdown
## Reflection Time

The task is complete - verification passed and code review is clean.

Please reflect on what you learned and update your knowledge files:

1. **Review** your current knowledge in `.claude/agents/{specialist}/`
2. **Add** new patterns, gotchas, or integration notes you discovered
3. **Update** any existing knowledge that evolved
4. **Remove** any knowledge that's now outdated
5. **Append** a Reflection section to the output file summarizing your learnings

If nothing new was learned, note that briefly and skip file updates.
```

---

# Part 3: Verification

## Specialist Runs Verification

The implementing specialist is responsible for running verification and fixing failures. This keeps context intact and ensures the specialist learns from its own mistakes.

**Verification layers** (specialist runs all 7):
1. `cargo check` - Compilation
2. `cargo fmt` - Auto-formatting
3. Simple guards - `./scripts/guards/run-guards.sh`
4. Unit tests - `cargo test --lib`
5. All tests - `cargo test`
6. Clippy - `cargo clippy -- -D warnings`
7. Semantic guards - `./scripts/guards/semantic/credential-leak.sh {files}`

The specialist iterates until all pass, documenting results in the output file.

## Orchestrator Validates (Trust but Verify)

After the specialist returns, the orchestrator re-runs verification:

```bash
./scripts/verify-completion.sh --verbose
```

**Exit codes**:
- 0 = Confirmed passing → Continue to code review
- 1 = Failed → Resume specialist to fix

This catches cases where the specialist skipped steps or didn't fully fix issues.

## Validating the Output File

The orchestrator also checks that the output file has required sections:

```bash
# Check output file exists and has verification results
grep -q "## Verification Results" docs/dev-loop-outputs/YYYY-MM-DD-*.md
```

If verification results are missing from the output file, the specialist skipped that step - resume to fix.

## Resume for Fixes

When verification fails after specialist returns:

```markdown
## Verification Failed

The orchestrator re-ran verification and it failed:

**Failed at**: {layer}
**Output**:
{failure details}

Your output file shows verification passed, but re-running shows failures.
Please investigate, fix the issues, re-run verification, and update the output file.
```

---

# Part 4: Code Review Integration

## Running Code Review

After verification passes (confirmed by orchestrator), run code review per `.claude/workflows/code-review.md`:

1. **Execute code review workflow**
   - Determine relevant ADRs and principles (same as implementation phase)
   - Run specialist reviewers in parallel (Code Reviewer, Security, Test, Observability)
   - Include Operations/Infrastructure specialists if relevant
   - **Save agent IDs** for each reviewer (needed for reflection later)

2. **Evaluate results**:
   - If NO findings: Continue to reflection ✓
   - If ANY findings: Resume implementing specialist to fix

3. **All findings are blocking** - BLOCKER, CRITICAL, MAJOR, MINOR, SUGGESTION all trigger fixes

## Saving Agent IDs

When invoking code review specialists, capture their agent IDs:

```
Security review: agent_id = abc123
Test review: agent_id = def456
Code Reviewer: agent_id = ghi789
```

These IDs are needed to resume each reviewer for reflection after the loop completes.

## Resume Implementing Specialist for Fixes

When code review has findings, **resume** the implementing specialist (don't invoke fresh):

```markdown
## Code Review Findings (Iteration {N} of 5)

Verification passed ✓

Code review found the following issues:

### 🔴 Security Specialist
1. **Missing rate limiting on new endpoint** - `src/handlers.rs:45`
   - Impact: DoS vulnerability
   - Fix: Add rate_limit middleware

### 🟡 Test Specialist
2. **Missing edge case test** - `src/auth.rs:120`
   - Missing: Test for expired token with valid signature
   - Fix: Add test case

### 🟢 Code Reviewer
3. **Inconsistent error message** - `src/errors.rs:30`
   - Current: "Invalid token"
   - Suggested: "Token validation failed" (matches ADR-0005)

Please address ALL findings:
1. Fix each issue
2. Re-run verification (all 7 layers)
3. Update the output file with fixes made
4. Return when complete
```

After the specialist fixes and returns, orchestrator re-validates and re-runs code review.

See `code-review.md` for full reviewer participation rules, synthesis process, and severity categories.

---

# Part 5: Collaboration Mode

## When to Enter Collaboration

After **5 failed attempts**, stop the loop and present to user:

```markdown
## Development Loop: Collaboration Needed

The specialist attempted this task 5 times but loop still fails.

### Current Status

{Remaining failures - could be verification OR code review}

### Attempt History

| Attempt | Result | Stage | What Failed | What Was Fixed |
|---------|--------|-------|-------------|----------------|
| 1 | FAIL | Verification | Compile error in auth.rs | Added missing import |
| 2 | FAIL | Verification | Guard: PII in logs | Used [REDACTED] for email |
| 3 | FAIL | Code Review | Security: missing rate limit | Added middleware |
| 4 | FAIL | Code Review | Test: missing edge case | Added test |
| 5 | FAIL | Code Review | Code quality issue | Still failing |

### Analysis

The current issue appears to be:
- {Brief analysis of why the specialist couldn't fix it}
- {Possible root causes}

### Suggested Next Steps

1. Review code review expectations - may be overly strict
2. Check if ADR design needs revision
3. Consider involving {other specialist} for fresh perspective
4. Manual debugging or pair programming
```

## Collaboration Options

Present to user:
1. **Provide guidance** - User gives hints, loop continues
2. **Adjust task** - Simplify or split task
3. **Involve another specialist** - Fresh perspective
4. **Debug together** - Interactive troubleshooting
5. **Accept with known issues** - Commit with documented limitations

---

# Part 6: Integration with Other Workflows

## Multi-Agent Debate

See: `multi-agent-debate.md`

When a task requires cross-cutting design decisions:

1. Identify debate topic keywords
2. Match to principle categories
3. Inject matched categories into Round 1 context
4. All specialists see same principles during debate

After debate produces ADR:
- ADR summary injected as "Design Context" in specialist prompt
- Development loop implements the debated design

## Standalone Code Review

The code review workflow (`code-review.md`) can also be run independently:

- Manual review of existing code
- PR reviews in CI/CD
- Spot checks outside the development loop

When run within the development loop, it uses the same reviewer set and principles as standalone mode.

---

# Part 7: Reflection

## Purpose

After a successful implementation (verification passed, code review clean), all involved specialists reflect on what they learned. This builds the specialist knowledge base over time and captures insights while context is fresh.

## Critical: Resume, Don't Re-invoke

Specialists must be **resumed** for reflection, not invoked fresh. This preserves their context so they can reflect on their actual experience, not a summary provided by the orchestrator.

```
# Correct - resume with agent ID
Task(resume="abc123", prompt="Time to reflect...")

# Wrong - fresh invocation loses context
Task(prompt="You just reviewed code for X, please reflect...")
```

## When to Reflect

Reflection happens **after code review is clean but before finalizing output**. This ensures:
- Specialists have intact memory of what they did
- Knowledge updates capture genuine learnings
- User sees complete picture before committing

## Who Reflects (All Resumed)

| Specialist | Agent ID Source | What They Reflect On |
|------------|-----------------|---------------------|
| Implementing specialist | Saved from step 2 | Implementation decisions, verification fixes |
| Security reviewer | Saved from step 4 | Security patterns observed, gaps found |
| Test reviewer | Saved from step 4 | Test coverage insights, testing patterns |
| Code Reviewer | Saved from step 4 | Code quality observations, idioms |
| Others (if participated) | Saved from step 4 | Domain-specific learnings |

## Reflection Prompt (for Resume)

Since the specialist is being resumed with full context, the prompt is simple:

```markdown
## Reflection Time

The task is complete - verification passed and code review is clean.

Please reflect on what you learned:

1. **Review** your current knowledge in `.claude/agents/{specialist}/`
2. **Add** new patterns, gotchas, or integration notes you discovered
3. **Update** any existing knowledge that evolved
4. **Remove** any knowledge that's now outdated
5. **Append** a Reflection section to the output file

Your knowledge files:
{Contents of .claude/agents/{specialist}/*.md, or "None yet - create them now"}

If nothing new was learned, note that briefly and skip file updates.
```

## Knowledge File Format

Each knowledge file follows a structured format:

```markdown
# Patterns (or Gotchas, or Integration)

## Pattern: Descriptive Title
**Added**: YYYY-MM-DD
**Related files**: `src/path/to/file.rs`, `src/another/file.rs`

Brief description of the pattern, gotcha, or integration note.
Keep it concise (2-4 sentences max).

## Pattern: Another Title
**Added**: YYYY-MM-DD
**Related files**: `src/file.rs`

Description here.
```

**Guidelines**:
- ~100 lines per file limit
- Each item has Added date and Related files
- Keep descriptions brief and actionable
- Use H2 headers for each item

## Types of Knowledge Updates

| Routine Updates | Significant Updates |
|-----------------|---------------------|
| Adding a gotcha from a mistake made | Changing fundamental approach patterns |
| Updating a pattern to match current code | Adding new knowledge categories |
| Removing knowledge about deleted code | Anything affecting security behavior |
| Typo/clarification fixes | Contradicting existing ADRs |

**Heuristic**: Most updates should be "learning from this task" - capturing patterns and gotchas. If you find yourself "rethinking how we do things", that may warrant discussion before updating.

## Approval Flow

1. **Reflection runs** after code review is clean
2. **Specialists update knowledge files directly** (create/modify as needed)
3. **Changes appear in git diff** alongside implementation
4. **User reviews everything** when exiting the loop
5. **User commits** when satisfied (implementation + knowledge updates together)

**Note**: Knowledge file changes are just regular file changes - the user sees them in the diff and can approve/reject like any other change.

## Bootstrap Behavior

When a specialist reflects for the first time (no knowledge files exist):

1. Specialist creates `.claude/agents/{specialist}/` directory
2. Creates initial `patterns.md`, `gotchas.md`, `integration.md` files
3. Populates with knowledge based on existing code patterns and the task just completed
4. User sees new files in git diff and can review/approve

## Pruning During Reflection

When a specialist removes or significantly changes code, they should identify related knowledge to remove:

```markdown
### Removals

**File**: gotchas.md
**Item**: "Legacy OAuth Token Format"
**Reason**: Removed legacy OAuth support in this task. The `parse_legacy_token()`
function no longer exists, so this gotcha is obsolete.
```

The orchestrator verifies the referenced code is actually gone before approving removal.

## Example Reflection Output

```markdown
### Additions

**Category**: patterns
**Title**: JWT Clock Skew Handling
**Description**: When validating JWTs, use the configurable clock skew tolerance
from config (default 300 seconds per NIST SP 800-63B). See `src/crypto/jwt.rs:validate()`.

### Updates

**File**: integration.md
**Item**: "Calling Auth Controller"
**Change**: Updated to note that AC now returns structured error responses with
error codes, not just HTTP status. Update client code to parse error body.

### Removals

None.
```

---

# Part 8: Output Documentation

## Purpose

The output file serves as:
- **Proof-of-work** - Evidence that the specialist ran all required steps
- **Audit trail** - Record of the development process
- **Historical reference** - Patterns in what works/doesn't

## Shared Responsibility

| Section | Written By | When |
|---------|------------|------|
| Task Overview | Specialist | During implementation |
| Implementation Summary | Specialist | During implementation |
| Files Modified | Specialist | During implementation |
| Verification Results (7 layers) | Specialist | After each verification run |
| Issues Encountered | Specialist | As issues arise |
| Reflection | Each specialist | During reflection step |
| Code Review Results | Orchestrator | After code review |
| Final Validation | Orchestrator | Before presenting to user |

## Output Location

`docs/dev-loop-outputs/YYYY-MM-DD-{task-slug}.md`

## Template

Use the template at `docs/dev-loop-outputs/_template.md`

## Orchestrator Validation

Before presenting to user, orchestrator validates:

```bash
# Required sections from specialist
grep -q "## Task Overview" $OUTPUT_FILE
grep -q "## Implementation Summary" $OUTPUT_FILE
grep -q "## Verification Results" $OUTPUT_FILE  # Proof verification was run

# If any missing, resume specialist to complete
```

If verification results are missing, the specialist skipped that step - resume to fix.

## Orchestrator Appends

After validation, orchestrator appends:
- Code review results (from all reviewers)
- Final validation timestamp
- Any orchestrator-level observations

---

# Part 9: State Checkpointing

## Purpose

When orchestrator context compresses mid-loop, critical state can be lost:
- Implementing specialist agent ID (needed to resume for fixes/reflection)
- Code review specialist agent IDs (needed for reflection)
- Current iteration count
- What steps have been completed

The dev-loop output file serves as a checkpoint to recover this state.

## Loop State Section

The orchestrator maintains a `## Loop State (Internal)` section in the output file:

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
```

**Field definitions**:
- `Implementing Agent`: Agent ID of the specialist doing the implementation
- `Current Step`: One of `implementation`, `validation`, `code_review`, `reflection`, `complete`
- `Iteration`: Current iteration number (1-5)
- `{Specialist} Reviewer`: Agent IDs for each code reviewer (added during code review step)

## When to Update

| Event | Action |
|-------|--------|
| Invoke implementing specialist | Write Loop State with agent ID, step=`implementation`, iteration=1 |
| Specialist returns | Update step to `validation` |
| Validation passes | Update step to `code_review` |
| Invoke each reviewer | Add reviewer agent ID to Loop State |
| Code review clean | Update step to `reflection` |
| Code review has findings | Increment iteration, update step to `implementation` |
| All reflection complete | Update step to `complete` |

## Recovering from Compression

When the orchestrator's context is compressed (you'll notice you don't remember earlier parts of the conversation):

1. **Read the output file** to find the Loop State section
2. **Resume from current step** using the stored agent IDs
3. **Don't re-run completed steps** - trust the output file's record

Example recovery:
```
Orchestrator notices context was compressed.
Reads output file → Loop State shows step=code_review, iteration=2
Reads agent IDs for reviewers
Continues with code review step using stored IDs
```

## Ownership Rules

- **Only the orchestrator** edits the Loop State section
- Specialists write other sections (Task Overview, Verification Results, etc.)
- This prevents conflicts when both are editing the same file

## Orchestrator Checklist

### Before Each State Transition

After completing any step, **immediately** update the Loop State in the output file:

1. **After invoking implementing specialist**:
   - Write Loop State with agent ID
   - Set Current Step to `implementation`
   - Set Iteration to `1`

2. **After specialist returns**:
   - Update Current Step to `validation`
   - Re-run verification

3. **After validation passes**:
   - Update Current Step to `code_review`
   - Invoke code reviewers

4. **When invoking each code reviewer** ⚠️ CRITICAL:
   - **Immediately** update Loop State with reviewer agent ID
   - Do NOT wait until all reviewers are done
   - This is the step most often skipped - reviewer IDs are needed for reflection

5. **After code review is clean**:
   - Update Current Step to `reflection`
   - Resume implementing specialist for reflection
   - Resume each reviewer for reflection (using saved agent IDs)

6. **After all reflections complete**:
   - Update Current Step to `complete`
   - Update Duration in output file header
   - Finalize output file

### Before Switching Tasks

If user requests a different task while a dev-loop is in progress:

1. Check Loop State in the current output file
2. If Current Step is NOT `complete`:
   - Ask user: "We have an incomplete dev-loop at step '{step}'. Complete it first or pause?"
   - If pause: Note the state for later resumption
   - If complete: Finish remaining steps first

### Common Mistakes to Avoid

- ❌ Forgetting to save reviewer agent IDs when invoking code review
- ❌ Skipping reflection step after code review is clean
- ❌ Switching to new user request without checking if loop is complete
- ❌ Leaving Loop State at `code_review` after reviewers approve

---

# Quick Reference

## Starting the Loop

1. Match task → categories (see mapping above)
2. Load specialist knowledge files if they exist
3. Build prompt with principles + knowledge + verification responsibilities
4. Announce: "Starting development loop (specialist-owned verification, max 5 iterations)"
5. Invoke implementing specialist
6. **Save agent ID** for later resume

## After Specialist Returns

1. **Re-run verification** (trust but verify): `./scripts/verify-completion.sh`
2. **Validate output file** has required sections
3. If verification FAIL → Resume specialist to fix
4. If output file incomplete → Resume specialist to complete
5. If all good → Run code review

## Code Review

1. Run reviewers in parallel (Security, Test, Code Reviewer, etc.)
2. **Save agent IDs** for each reviewer
3. If ANY findings → Resume implementing specialist with findings → back to validation
4. If CLEAN → Continue to reflection

## Reflection (All Specialists Resumed)

1. **Resume** implementing specialist → reflect → update knowledge files
2. **Resume** each code review specialist → reflect → update knowledge files
3. Each specialist appends reflection to output file

## Finalize

1. Orchestrator appends code review results to output file
2. Validate all required sections present
3. Announce loop completion
4. User reviews git diff (implementation + knowledge files + output doc)
5. User commits when satisfied

## Agent IDs to Track

| Agent | When Saved | Used For |
|-------|------------|----------|
| Implementing specialist | Step 2 initial invoke | Fix cycles, reflection |
| Security reviewer | Code review | Reflection |
| Test reviewer | Code review | Reflection |
| Code Reviewer | Code review | Reflection |
| Others | Code review | Reflection |

## Verification Commands

```bash
# Full verification (default)
./scripts/verify-completion.sh

# Quick feedback during development
./scripts/verify-completion.sh --layer quick

# Machine-readable output
./scripts/verify-completion.sh --format json
```

## Categories Shorthand

- `crypto` - secrets, keys, hashing, encryption
- `jwt` - token validation, claims, expiry
- `logging` - no secrets in logs, structured format
- `queries` - parameterized SQL, no injection
- `errors` - no panics, proper types
- `input` - validation, limits, sanitization
- `testing` - test ownership, three tiers, determinism
- `concurrency` - actor pattern, message passing
- `api-design` - URL versioning, deprecation
- `observability` - privacy-by-default, metrics, spans

---

# Related Workflows

| Workflow | When to Use | Relationship to Loop |
|----------|-------------|---------------------|
| `multi-agent-debate.md` | Cross-cutting design | Happens BEFORE loop, produces ADR |
| `code-review.md` | Quality gate | Integrated INTO loop (step 4), also usable standalone |
| `orchestrator-guide.md` | General orchestration | This loop is a key subprocess |
| `process-review-record.md` | Process failures | Use when loop reveals coordination gaps |
