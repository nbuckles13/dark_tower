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
│     └─→ Run specialist reviewers in parallel (incl. DRY)        │
│     └─→ Save agent IDs for later resume                         │
│     └─→ Blocking findings → Resume impl specialist → fix → 3    │
│        • All findings block EXCEPT TECH_DEBT severity           │
│        • TECH_DEBT: Document in Tech Debt section, continue     │
│     └─→ If no blocking findings: Continue to reflection         │
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
3. **Specialist knowledge** - From `docs/specialist-knowledge/{specialist}/` (if exists)
4. **Design context** - ADR summary if from debate
5. **Task context** - The actual task description and existing patterns

## Specialist Knowledge Files

Each specialist may have accumulated knowledge in `docs/specialist-knowledge/{specialist}/`:
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

## Initial Invocation

When invoking an implementing specialist, include these sections in order:

1. **Specialist definition** - From `.claude/agents/{specialist}.md`
2. **Implementation mode header** - "You are being invoked to IMPLEMENT" + expected behavior
3. **Project principles** - Matched category files from `docs/principles/`
4. **Accumulated knowledge** - From `docs/specialist-knowledge/{specialist}/*.md` (if exists)
5. **Design context** - ADR summary if from debate
6. **Existing patterns** - Relevant code snippets
7. **Task description** - The actual task
8. **Responsibilities** - Implement → verify (7 layers) → fix → write output + checkpoint → return

**Key instructions to include**:
- If requirements unclear: Return to orchestrator with questions (don't guess)
- Run all 7 verification layers and fix failures before returning
- Write output to `docs/dev-loop-outputs/YYYY-MM-DD-{task-slug}/main.md`
- Write checkpoint to `docs/dev-loop-outputs/YYYY-MM-DD-{task-slug}/{your-name}.md`

**Templates**: See `docs/dev-loop-outputs/_template/` for output and checkpoint formats.

## Resume for Fixes

When resuming to fix code review findings, provide:
- Iteration number (N of 5)
- List of findings with severity, file:line, description, suggested fix
- Instruction: Fix all → re-run verification → update output → return

## Resume for Reflection

When resuming for reflection (after code review clean):
- Instruction: Review knowledge files → add/update/remove entries → append reflection to output
- Reference: `docs/specialist-knowledge/{specialist}/*.md`

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

2. **Evaluate results** (severity-based blocking):
   - **All severities block EXCEPT TECH_DEBT**
   - TECH_DEBT findings: Document in Tech Debt section, continue (don't block)
   - If NO blocking findings: Continue to reflection ✓
   - If blocking findings exist: Resume implementing specialist to fix

3. **Who uses TECH_DEBT severity**:
   - **DRY Reviewer**: Cross-service duplication (candidates for extraction to common)
   - **Code Reviewer**: Temporary code (scaffolding, test endpoints, placeholders)

**See**: ADR-0019 for DRY Reviewer rationale

## Saving Agent IDs

When invoking code review specialists, capture their agent IDs:

```
Security review: agent_id = abc123
Test review: agent_id = def456
Code Reviewer: agent_id = ghi789
```

These IDs are needed to resume each reviewer for reflection after the loop completes.

## Resume Implementing Specialist for Fixes

When code review has findings, **resume** the implementing specialist (don't invoke fresh). See Part 2 "Resume for Fixes" for prompt format.

**After fixes**: Orchestrator MUST re-run verification before re-running code review. This ensures fixes don't introduce new issues.

See `code-review.md` for full reviewer participation rules and severity categories.

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

After code review is clean, all specialists reflect on learnings. This builds the knowledge base while context is fresh.

## Critical: Resume, Don't Re-invoke

Specialists must be **resumed** (not invoked fresh) to preserve context. See Part 2 "Resume for Reflection" for prompt format.

## Who Reflects

| Specialist | Agent ID Source |
|------------|-----------------|
| Implementing specialist | Saved from step 2 |
| All reviewers | Saved from step 4 |

## What Specialists Do

1. Review knowledge in `docs/specialist-knowledge/{specialist}/`
2. Add/update/remove entries based on learnings
3. Append reflection summary to checkpoint file

**Knowledge file format**: See existing files in `docs/specialist-knowledge/*/` for examples.

**Guidelines**: ~100 lines per file, each entry has Added date + Related files, keep descriptions to 2-4 sentences.

## Bootstrap Behavior

First-time reflection: Specialist creates `docs/specialist-knowledge/{specialist}/` with initial `patterns.md`, `gotchas.md`, `integration.md`.

## Approval Flow

Knowledge file changes appear in git diff alongside implementation. User reviews and commits everything together.

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

```
docs/dev-loop-outputs/YYYY-MM-DD-{task-slug}/
├── main.md              # Main output (orchestrator owns Loop State)
├── {specialist}.md      # Implementing specialist checkpoint
└── {reviewer}.md        # One checkpoint per code reviewer
```

## Templates

- Main output: `docs/dev-loop-outputs/_template/main.md`
- Specialist checkpoint: `docs/dev-loop-outputs/_template/specialist.md`

## Orchestrator Validation

Before presenting to user, orchestrator validates:

```bash
# Check output directory exists
OUTPUT_DIR="docs/dev-loop-outputs/YYYY-MM-DD-{task-slug}"
test -d "$OUTPUT_DIR"

# Required sections in main.md
grep -q "## Task Overview" "$OUTPUT_DIR/main.md"
grep -q "## Implementation Summary" "$OUTPUT_DIR/main.md"
grep -q "## Verification Results" "$OUTPUT_DIR/main.md"

# Checkpoint file exists for implementing specialist
test -f "$OUTPUT_DIR/{specialist}.md"

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

1. Run reviewers in parallel (Security, Test, Code Reviewer, DRY, etc.)
2. **Save agent IDs** for each reviewer
3. Evaluate blocking findings (severity-based):
   - All findings block EXCEPT TECH_DEBT severity
   - TECH_DEBT findings: Document in Tech Debt section, continue
4. **After specialist fixes**: Re-run verification (`./scripts/verify-completion.sh`) → re-run code review
5. If no blocking findings → Continue to reflection

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

---

# Part 10: Session Restore

## Purpose

If a session is interrupted (computer restart, context compression, process kill), agent context is lost. Per-specialist checkpoint files enable meaningful recovery by capturing working notes as specialists work.

## Checkpoint Files

Each specialist writes to their own checkpoint file during work:

```
docs/dev-loop-outputs/YYYY-MM-DD-{task}/
├── main.md                    # Main output (orchestrator owns)
├── {implementing-specialist}.md  # Implementing specialist's working notes
└── {reviewer}.md              # Each reviewer's observations
```

**Checkpoint content** (written as specialist works):
- **Patterns Discovered** - What approaches worked well
- **Gotchas Encountered** - What was tricky, what to warn others about
- **Key Decisions** - Choices made and why
- **Observations** - What informed the review verdict (reviewers)
- **Status** - Current step, verdict, timestamp

## Restore Procedure

When starting a new session, check for incomplete dev-loops:

1. **Scan** `docs/dev-loop-outputs/` for directories
2. **Check** each `main.md` for Loop State with `Current Step != complete`
3. **If found**, offer restore to user

### Restore Prompt

```
Found incomplete dev-loop: {task-slug}
- Current step: {step}
- Iteration: {iteration}
- Implementing specialist: {name}

Restore and continue? (Specialists will be re-invoked with checkpoint context)
```

### Restore Context Template

When restoring a specialist, inject their checkpoint:

```markdown
# Context Recovery for {Specialist}

You are continuing a dev-loop that was interrupted. Here's your previous context:

## Your Previous Working Notes

{paste from checkpoint file: Patterns, Gotchas, Decisions, Observations}

## Current Loop State

- Step: {current_step}
- Iteration: {iteration}

## What's Already Complete

{summary from main.md: Task Overview, Implementation Summary if present}

## Your Task

Continue from where you left off. Based on your working notes, {specific instruction for current step}.
```

## Validation Before Step Transitions

Orchestrator validates checkpoint files exist before advancing:

| Transition | Validation |
|------------|------------|
| Implementation → Validation | Implementing specialist checkpoint has Patterns/Gotchas sections |
| Code Review → Reflection | All reviewer checkpoints have Observations sections |
| Reflection → Complete | All specialists have updated Status to reflection complete |

## Limitations

Restored specialists have checkpoint context but not full memory. The restore is "good enough" to continue meaningful work, but may miss nuances from the original session. This is acceptable - the alternative is starting over completely.
