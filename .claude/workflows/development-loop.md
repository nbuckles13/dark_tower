# Development Loop Workflow

The Development Loop is the primary workflow for implementing features. It uses an interactive model where the **user orchestrates** and **Claude executes steps**.

---

## Roles

| Role | Who | Responsibilities |
|------|-----|------------------|
| **Orchestrator** | User | Initiates loop, approves steps, decides when to proceed |
| **Step-Runner** | Claude | Prepares prompts, spawns specialists, runs validation |
| **Specialist** | Task agents | Domain expertise, implements code, reviews changes |

---

## When to Use

| Scenario | Use Loop? | Notes |
|----------|-----------|-------|
| Implement new feature | Yes | Standard flow |
| Bug fix | Yes | Unless trivial one-liner |
| Refactoring | Yes | Tests catch regressions |
| Documentation only | No | No verification needed |
| Exploration/research | No | No code to verify |

---

## The Process

### Step 0: Initiation

**User says**: "Let's work on task X in a dev-loop"

**Claude (step-runner)**:
1. Creates output directory: `docs/dev-loop-outputs/YYYY-MM-DD-{task-slug}/`
2. Creates `main.md` with task overview and loop state
3. Identifies implementing specialist
4. Matches task to principle categories
5. Prepares specialist prompt and shows user:
   - Matched principles (file paths)
   - Task description
   - Expected output format

**User reviews** and says: "That looks good, run step 1"

---

### Step 1: Implementation

**Claude (step-runner)**:
1. Spawns implementing specialist via Task tool
2. Specialist prompt includes:
   - Specialist definition (from `.claude/agents/{specialist}.md`)
   - Accumulated knowledge (from `docs/specialist-knowledge/{specialist}/`)
   - Matched principles (from `docs/principles/`)
   - Task description
3. Waits for specialist to complete
4. Updates `main.md` with implementation summary
5. Reports results to user

**Specialist responsibilities**:
- Implement the task
- Run verification (cargo check, tests, clippy)
- Create checkpoint file at `{output_dir}/{specialist}.md`
- End response with structured output block

**User reviews** and says: "That looks good, run step 2"

---

### Step 2: Validation

**Claude (step-runner)** runs directly (no Task spawn):
1. Layer 1: `cargo check --workspace`
2. Layer 2: `cargo fmt --all --check`
3. Layer 3: `./scripts/guards/run-guards.sh`
4. Layer 4: `cargo test --workspace --lib`
5. Layer 5: `cargo test --workspace` (all tests)
6. Layer 6: `cargo clippy --workspace --lib --bins -- -D warnings`
7. Layer 7: Semantic guards (if applicable)

Updates `main.md` with verification results.

**If failed**: Reports which layer failed, user decides whether to resume specialist or fix directly.

**User reviews** and says: "That looks good, run step 3"

---

### Step 3: Code Review

**Claude (step-runner)**:
1. Spawns 4 reviewers in parallel via Task tool:
   - Security specialist
   - Test specialist
   - Code-reviewer specialist
   - DRY-reviewer specialist
2. Each reviewer gets: files changed, implementation summary
3. Collects verdicts and findings
4. Updates `main.md` with code review results

**Verdicts**:
- `APPROVED` - No issues
- `FINDINGS` - Issues to fix (blocks if Security/Test/Code-reviewer)
- `TECH_DEBT` - DRY-reviewer non-blocking findings

**If findings**: User decides whether to resume implementing specialist or fix directly.

**User reviews** and says: "That looks good, run step 4"

---

### Step 4: Reflection

**Claude (step-runner)**:
1. Resumes implementing specialist for reflection
2. Specialist reviews their work and updates knowledge files:
   - `docs/specialist-knowledge/{specialist}/patterns.md`
   - `docs/specialist-knowledge/{specialist}/gotchas.md`
   - `docs/specialist-knowledge/{specialist}/integration.md`
3. Optionally resumes reviewers for their reflections
4. Updates `main.md` with lessons learned

**User reviews** and says: "Complete the loop"

---

### Step 5: Complete

**Claude (step-runner)**:
1. Updates loop state to `complete`
2. Summarizes the dev-loop outcome

---

## Loop State

Tracked in `main.md`:

```markdown
## Loop State (Internal)

| Field | Value |
|-------|-------|
| Implementing Specialist | `{specialist-name}` |
| Current Step | `{step}` |
| Iteration | `{n}` |
| Implementing Agent ID | `{id}` |
| Security Reviewer ID | `{id or pending}` |
| Test Reviewer ID | `{id or pending}` |
| Code Reviewer ID | `{id or pending}` |
| DRY Reviewer ID | `{id or pending}` |
```

---

## Iteration (Fixing Issues)

When validation fails or code review has findings:

1. User decides: "Resume specialist to fix" or "I'll fix directly"
2. If resuming: Claude resumes the specialist via Task with findings
3. Increment iteration counter (max 5)
4. Return to validation step

---

## Specialist Prompt Structure

When spawning a specialist, include:

```markdown
{Contents of .claude/agents/{specialist}.md}

## Principles

{Contents of matched docs/principles/*.md files}

## Accumulated Knowledge

{Contents of docs/specialist-knowledge/{specialist}/*.md if exists}

## Task

{Task description - VERBATIM from user}

## Your Responsibilities

1. Implement the task
2. Run verification (cargo check, test, clippy)
3. Create checkpoint at `{output_dir}/{specialist}.md`
4. End with structured output:

---RESULT---
STATUS: SUCCESS or FAILURE
SUMMARY: Brief description of what was done
FILES_MODIFIED: Comma-separated list
TESTS_ADDED: Number (0 if none)
VERIFICATION: PASSED or FAILED
ERROR: Error message if FAILURE, or "none"
---END---
```

---

## What Claude Shows Before Each Step

Before spawning specialists, Claude shows:
- **Matched principles**: File paths that will be injected
- **Task prompt**: Exactly what the specialist will see

User can adjust before approving.

---

## Related Files

| File | Purpose |
|------|---------|
| `development-loop/step-validation.md` | Detailed validation layer instructions |
| `development-loop/session-restore.md` | Recovery from interrupted loops |
| `code-review.md` | Code review process details |
| `docs/dev-loop-outputs/_template/` | Output file templates |
