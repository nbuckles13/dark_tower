---
name: dev-loop-fix
description: Resume implementing specialist to fix validation failures or code review findings.
disable-model-invocation: true
---

# Dev-Loop Fix

Resume the implementing specialist with:
- Specific findings to address (from validation or code review)
- Current iteration count
- Instructions to fix, re-verify, and return

Used after:
- `/dev-loop-validate` fails
- `/dev-loop-review` returns findings

## Arguments

```
/dev-loop-fix [output-dir]
```

- **output-dir** (optional): Override auto-detected output directory

## Instructions

### Step 1: Locate Active Dev-Loop

If output-dir not provided, auto-detect:

1. List directories in `docs/dev-loop-outputs/` (excluding `_template`)
2. Filter to `Current Step` = `fix`
3. If exactly one: use it
4. If multiple: ask user which one
5. If none: error - "No dev-loop in fix state. Run `/dev-loop-validate` or `/dev-loop-review` first."

### Step 2: Check Iteration Count

Read current iteration from Loop State. Max is 5.

If iteration >= 5:

```
**Max Iterations Reached**

This dev-loop has reached 5 iterations without passing.

Options:
1. Review the failures manually
2. Restart with a different approach
3. Escalate to user for guidance

Current failures:
{list from validation or code review}
```

Wait for user input.

### Step 3: Increment Iteration

Update Loop State:

| Field | Value |
|-------|-------|
| Iteration | `{current + 1}` |

### Step 4: Collect Findings

Based on Current Step, collect findings:

#### If Current Step = `validation`

Read the "Dev-Loop Verification Steps" section for failures:

```
**Validation Failures**:
- Layer {N}: {layer name}
- Error: {error details}
- Files affected: {list}
```

#### If Current Step = `code_review`

Read the "Code Review Results" section for findings:

```
**Code Review Findings**:

1. [{severity}] {reviewer}: {finding}
   File: {path:line}
   Fix: {suggested fix}

2. [{severity}] {reviewer}: {finding}
   File: {path:line}
   Fix: {suggested fix}
...
```

Filter out TECH_DEBT findings (they don't block).

### Step 5: Get Implementing Agent ID

Read from Loop State:

| Field | Value |
|-------|-------|
| Implementing Agent | `{agent_id}` |

### Step 6: Build Fix Prompt

```markdown
## Fix Required - Iteration {N} of 5

The implementation has {validation failures | code review findings} that need to be fixed.

### Findings to Address

{findings list from Step 4 - VERBATIM}

### Your Responsibilities

1. **Fix all findings** listed above
2. **Re-run all 7 verification layers**:
   - Layer 1: `cargo check --workspace`
   - Layer 2: `cargo fmt --all --check`
   - Layer 3: `./scripts/guards/run-guards.sh`
   - Layer 4: `./scripts/test.sh --workspace --lib`
   - Layer 5: `./scripts/test.sh --workspace`
   - Layer 6: `cargo clippy --workspace -- -D warnings`
   - Layer 7: Semantic guards on modified files
3. **Update main.md** with:
   - Updated Implementation Summary
   - Updated Files Modified
   - New verification results
   - **Note**: Do NOT modify `Implementing Agent` in Loop State - that is managed by the orchestrator
4. **Update your checkpoint** with fix details
5. **Return** with structured output

**CRITICAL**: All cargo commands MUST use `--workspace`.

### Output Directory

{output_dir}/

---

## Expected Return Format

```
status: success | failed
fixes_applied: [list of fixes made]
verification_passed: true | false
files_modified: [list]
error: {if failed}
```
```

### Step 7: Resume Specialist

Use Task tool with `resume` parameter:

```
Task tool parameters:
- subagent_type: "general-purpose"
- resume: "{agent_id}"
- prompt: {fix prompt from Step 6}
```

#### If Resume Fails

Fall back to checkpoint injection:

1. Read: `{output_dir}/{specialist}.md`
2. Invoke fresh agent with checkpoint:

```markdown
# Context Recovery for {Specialist}

You are continuing a dev-loop fix iteration. Here's your previous context:

## Your Previous Working Notes

{checkpoint file content}

## Current Task

{fix prompt from Step 6}
```

### Step 8: Capture Results

After Task returns:

- If specialist found new agent ID (fresh spawn): update Loop State
- Record fixes applied

### Step 9: Update main.md

Add to "Issues Encountered & Resolutions" section:

```markdown
### Iteration {N}: {Fix Summary}
**Problem**: {validation/review findings}
**Resolution**: {fixes applied}
```

### Step 10: Report Results

#### If Fixes Applied Successfully

Update Loop State:

| Field | Value |
|-------|-------|
| Current Step | `validation` |

```
**Fixes Applied - Iteration {N}**

Specialist: {name}
Fixes made:
{list of fixes}

**Next step**: Run `/dev-loop-validate`
```

#### If Fixes Failed

```
**Fix Attempt Failed - Iteration {N}**

Error: {error details}

Options:
1. Retry with `/dev-loop-fix`
2. Review manually and make changes
3. Restart the dev-loop

**Next step**: Your choice
```

## Critical Constraints

- **VERBATIM findings**: Pass findings exactly as reported, don't paraphrase
- **Increment iteration**: Track iteration count (max 5)
- **Resume, don't re-invoke**: Use existing agent ID when possible
- **Filter TECH_DEBT**: Tech debt findings don't require fixes
- **--workspace flag**: All cargo commands must use --workspace

## State Machine

```
implementation
     ↓
validation ←─────────┐
     ↓ pass   ↓ fail │
code_review   fix ───┘
     ↓ pass   ↓ fail
reflection    fix ───→ validation → code_review → ...
```

| Current Step | Event | Next Step |
|--------------|-------|-----------|
| `validation` | passes | `code_review` |
| `validation` | fails | `fix` |
| `code_review` | APPROVED | `reflection` |
| `code_review` | REQUEST_CHANGES | `fix` |
| `fix` | fix completes | `validation` |

---

**Next step**: Always run `/dev-loop-validate` after fix (then `/dev-loop-review` after validation passes)
