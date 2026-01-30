---
name: dev-loop-implement
description: Spawn the implementing specialist with injected context. Run after /dev-loop-init.
disable-model-invocation: true
---

# Dev-Loop Implement

Spawn the implementing specialist with:
- Specialist definition
- Matched principles
- Accumulated knowledge files
- Task objective and detailed requirements

The specialist implements the task, runs 7-layer verification, and writes checkpoint files.

## Arguments

```
/dev-loop-implement [output-dir]
```

- **output-dir** (optional): Override auto-detected output directory

## Instructions

### Step 1: Locate Active Dev-Loop

If output-dir not provided, auto-detect:

1. Run `./scripts/workflow/dev-loop-status.sh --active-only`
2. Filter output to loops with `Current Step` in (`init`, `planning`, `implementation`)
3. If exactly one: use it
4. If multiple: ask user which one
5. If none: error - "No active dev-loop. Run `/dev-loop-init` first."

Read the `main.md` to get:
- Task objective (from Task Overview > Objective)
- Detailed requirements (from Task Overview > Detailed Requirements)
- Implementing Specialist name (from Loop State)
- Implementing Agent ID (from Loop State - may be set from planning)
- Matched principles (from Matched Principles section)
- Planning Proposal section (if exists - from `/dev-loop-plan`)

### Step 2: Determine Resume vs Fresh Spawn

Check the `Implementing Agent` field from Loop State:

**If agent ID exists (not `pending`)**: Planning was used
- Will resume the existing agent with full context
- The agent already explored the codebase during planning
- Inject the approved plan as implementation instructions

**If agent ID is `pending`**: Fresh spawn needed
- Standard flow, no planning was done
- Will spawn new agent with full prompt

Update `main.md` Loop State:

| Field | Value |
|-------|-------|
| Current Step | `implementation` |
| Implementing Agent | `{keep existing if resuming, or pending if fresh}` |

### Step 3: Build Specialist Prompt

Read and concatenate these files in order:

1. **Specialist definition**: `.claude/agents/{specialist}.md`
2. **Principles**: Each file from `docs/principles/{category}.md`
3. **Knowledge files** (if exist): `docs/specialist-knowledge/{specialist}/*.md`

Then add the task section:

```markdown
{specialist definition content}

---

## Project Principles

{content of each matched principle file, separated by ---}

---

## Accumulated Knowledge

{content of patterns.md, gotchas.md, integration.md if they exist}
{or "No accumulated knowledge yet. You will create knowledge files during reflection."}

---

## Task

{task objective from main.md}

### Detailed Requirements

{detailed requirements section from main.md - this contains the full context needed to complete the task, including code examples, file locations, and acceptance criteria}

---

## Your Responsibilities

1. **Implement** the task as described
2. **Run all 7 verification layers** and fix any failures before returning:
   - Layer 1: `cargo check --workspace`
   - Layer 2: `cargo fmt --all --check`
   - Layer 3: `./scripts/guards/run-guards.sh`
   - Layer 4: `./scripts/test.sh --workspace --lib` (unit tests)
   - Layer 5: `./scripts/test.sh --workspace` (all tests)
   - Layer 6: `cargo clippy --workspace -- -D warnings`
   - Layer 7: `./scripts/guards/run-guards.sh --semantic`
3. **Write checkpoint** to `{output_dir}/{specialist}.md` with:
   - Patterns Discovered
   - Gotchas Encountered
   - Key Decisions
   - Current Status
4. **Update main.md** with:
   - Implementation Summary section
   - Files Modified section
   - Dev-Loop Verification Steps section (results of 7 layers)
   - **Note**: You may update `Current Step` in Loop State to "validation" when complete, but do NOT modify `Implementing Agent` - that is managed by the orchestrator
5. **Return** with structured output

**CRITICAL**: Use `--workspace` for all cargo commands. Changes in one crate can break others.

**If requirements are unclear**: Return to ask questions. Do not guess.

---

## Output Directory

Write files to: `{output_dir}/`
- Checkpoint: `{output_dir}/{specialist}.md`
- Main output: `{output_dir}/main.md` (update, don't overwrite)

---

## Expected Return Format

When complete, return:

```
status: success | failed
files_created: [list]
files_modified: [list]
checkpoint_exists: true | false
verification_passed: true | false
error: {if failed, explanation}
```
```

### Step 4: Invoke Specialist

#### If Resuming from Planning (agent ID exists and not `pending`)

Resume the existing agent with implementation instructions:

```
Task tool parameters:
- subagent_type: "general-purpose"
- description: "Implement: {first 30 chars of task}"
- prompt: {resume prompt - see below}
- resume: "{existing_agent_id}"
```

**Resume prompt**:

```markdown
## Implementation Phase

Planning is complete. Proceed with implementation based on the approved plan.

{Include Planning Proposal section from main.md if available}

### Your Responsibilities

1. **Implement** the task as planned
2. **Run all 7 verification layers** and fix any failures before returning:
   - Layer 1: `cargo check --workspace`
   - Layer 2: `cargo fmt --all --check`
   - Layer 3: `./scripts/guards/run-guards.sh`
   - Layer 4: `./scripts/test.sh --workspace --lib` (unit tests)
   - Layer 5: `./scripts/test.sh --workspace` (all tests)
   - Layer 6: `cargo clippy --workspace -- -D warnings`
   - Layer 7: `./scripts/guards/run-guards.sh --semantic`
3. **Write checkpoint** to `{output_dir}/{specialist}.md`
4. **Update main.md** with implementation results

**CRITICAL**: You have full context from planning. Use it.
```

#### If Fresh Spawn (agent ID is `pending`)

Use the **Task tool** with `general-purpose` subagent_type and model `Opus`:

```
Task tool parameters:
- subagent_type: "general-purpose"
- description: "Implement: {first 30 chars of task}"
- prompt: {built prompt from Step 3}
```

**Why Task tool**: Task sub-agents inherit session permissions. Using `claude --print` would spawn a separate process that cannot get user approval.

### Step 5: Update Loop State

After Task tool returns:

**If fresh spawn**: Capture the agent ID from the response.

**If resumed**: Agent ID already set from planning.

Update `main.md` Loop State:

| Field | Value |
|-------|-------|
| Implementing Agent | `{agent_id}` (new or existing) |
| Current Step | `implementation` |

### Step 6: Verify Checkpoint Exists

Check that the specialist created their checkpoint:

```bash
test -f {output_dir}/{specialist}.md && echo "exists"
```

If checkpoint missing, report warning but continue.

### Step 7: Report Results

Based on specialist's return:

#### If Success

```
**Implementation Complete**

Specialist: {specialist-name}
Agent ID: {agent_id}
Files created: {count}
Files modified: {count}
Verification: {passed/failed}

Checkpoint: {output_dir}/{specialist}.md

**Next step**: Run `/dev-loop-validate`
```

#### If Failed

```
**Implementation Failed**

Specialist: {specialist-name}
Agent ID: {agent_id}
Error: {error details}

**Next step**: Review error and run `/dev-loop-fix` or restart with `/dev-loop-implement`
```

## Critical Constraints

- **Include full requirements**: Copy both Objective and Detailed Requirements from main.md - this is the source of truth for task context
- **No implementation by orchestrator**: If Task invocation fails, return error. Do NOT fall back to implementing yourself.
- **Checkpoint required**: Specialist must create checkpoint file
- **--workspace flag**: All cargo commands must use --workspace

## Verification Commands Reference

For specialist to run:

```bash
# Layer 1: Compilation
cargo check --workspace

# Layer 2: Formatting
cargo fmt --all --check

# Layer 3: Simple guards
./scripts/guards/run-guards.sh

# Layer 4: Unit tests
./scripts/test.sh --workspace --lib

# Layer 5: All tests (integration)
./scripts/test.sh --workspace

# Layer 6: Lints
cargo clippy --workspace -- -D warnings

# Layer 7: Semantic guards
./scripts/guards/run-guards.sh --semantic
```

---

**Next step**: Run `/dev-loop-validate`
