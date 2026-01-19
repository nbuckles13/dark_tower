# Specialist Invocation (for Step-Runners)

This file describes how step-runners invoke specialists during the dev-loop.

---

## How to Invoke

Use `claude --print` via Bash to invoke specialists.

**Why `claude --print` (not Task tool)**:
- Task sub-agents cannot spawn nested agents (Task tool not available to them)
- `claude --print` with `--allowedTools` gives specialists full tool access
- `--resume` enables iteration cycles without losing context
- `--output-format json` provides structured metadata (cost, turns, session_id)

**Command Structure**:

```bash
claude --print \
  --model opus \
  --output-format json \
  --allowedTools "Read,Edit,Write,Bash,Grep,Glob" \
  "$specialist_prompt"
```

**For iteration 2+ (fixing findings)**:

```bash
claude --print \
  --model opus \
  --output-format json \
  --allowedTools "Read,Edit,Write,Bash,Grep,Glob" \
  --resume "$session_id" \
  "$findings_prompt"
```

---

## Building the Specialist Prompt

Read and concatenate these inputs:

| Input | Source | Required |
|-------|--------|----------|
| Specialist definition | `.claude/agents/{specialist}.md` | Yes |
| Matched principles | `docs/principles/*.md` (paths provided by orchestrator) | Yes |
| Accumulated knowledge | `docs/specialist-knowledge/{specialist}/*.md` (if exists) | If exists |
| Task description | Provided by orchestrator | Yes |
| Findings to address | Provided by orchestrator (iteration 2+) | If iteration 2+ |

### Prompt Structure

```markdown
{contents of specialist definition file}

## Principles

{contents of each matched principle file}

## Accumulated Knowledge

{contents of patterns.md, gotchas.md, integration.md if they exist}

## Task

{task description - VERBATIM from orchestrator}

## Findings to Address

{findings - VERBATIM from orchestrator, or omit section if iteration 1}

## Your Responsibilities

1. Implement the task (or fix the findings)
2. Run all 7 verification layers
3. Fix any failures before returning
4. Write checkpoint to `{output_dir}/{specialist}.md`
5. Update `{output_dir}/main.md` with Implementation Summary

## Required Output Format

End your response with this exact structure:

---RESULT---
STATUS: SUCCESS or FAILURE
SUMMARY: Brief description of what was done
FILES_MODIFIED: Comma-separated list of files changed
TESTS_ADDED: Number of tests added (0 if none)
VERIFICATION: PASSED or FAILED (did all 7 layers pass?)
ERROR: Error message if FAILURE, or "none" if SUCCESS
---END---
```

---

## Parsing Specialist Response

The JSON output contains:

```json
{
  "session_id": "uuid-for-resume",
  "result": "specialist's full response including ---RESULT--- block",
  "num_turns": 5,
  "total_cost_usd": 0.15,
  "is_error": false
}
```

**Extract status from result**:

```bash
result=$(echo "$json_output" | jq -r '.result')
status=$(echo "$result" | grep "STATUS:" | awk '{print $2}')
session_id=$(echo "$json_output" | jq -r '.session_id')

if [ "$status" = "FAILURE" ]; then
  # Report failure to orchestrator
fi
```

**Important**: Do NOT trust `is_error` field - it's always `false`. Use the `STATUS:` line from the structured output block instead.

---

## Session Management

**Capture session_id** after first invocation and log it to `main.md`:

```markdown
## Session Tracking

| Specialist | Session ID | Status |
|------------|------------|--------|
| auth-controller | 9e956e47-4f9a-436a-88ad-5c60c80827ce | iteration-1 |
```

**Resume for iterations**: Use `--resume "$session_id"` for iteration 2+ to preserve context and reduce costs (prompt caching gives ~100x cost reduction).

---

## Critical Constraints

**Do NOT**:
- Paraphrase or summarize the task - pass it VERBATIM
- Add implementation suggestions or design guidance
- Specify function names, patterns, or architecture
- Write code yourself - the specialist writes code
- **Implement directly if specialist invocation fails** - return error to orchestrator instead

**DO**:
- Read and concatenate the files as specified
- Pass task and findings exactly as received from orchestrator
- Capture and log session_id for resume capability
- Parse the `---RESULT---` block for status
- Verify checkpoint exists after specialist returns
- **If invocation fails**: Return `status: failed` with error details. Do NOT fall back to implementing yourself.

**Why no fallback implementation**: Step-runners have full tool access, so they *could* implement directly. But this bypasses specialist expertise, principles injection, and creates confusion about who did the work. Always return errors to orchestrator.

---

## Checkpoint Verification

After specialist returns, verify:
1. `STATUS: SUCCESS` in the `---RESULT---` block
2. Checkpoint file exists at `{output_dir}/{specialist}.md`
3. Checkpoint has "Prompt Received" section filled in (for audit trail)

If checkpoint is missing or status is FAILURE, report failure to orchestrator.

---

## Specialist Checkpoint Template

Specialists should record what prompt they received. See `docs/dev-loop-outputs/_template/specialist.md` for the template with "Prompt Received" section.

---

## JSON Output Reference

Full JSON structure from `claude --print --output-format json`:

| Field | Description | Use |
|-------|-------------|-----|
| `session_id` | UUID for session resume | Log to main.md, use with `--resume` |
| `result` | Specialist's full response | Parse for `---RESULT---` block |
| `num_turns` | Number of tool calls | Audit/debugging |
| `total_cost_usd` | Cost of invocation | Cost tracking |
| `duration_ms` | Wall-clock time | Performance monitoring |
| `is_error` | Always false (unreliable) | Do NOT use for error detection |
| `permission_denials` | Array of denied permissions | Check if non-empty = permission issue |
