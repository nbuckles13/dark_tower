# Specialist Invocation

This file describes how Claude (step-runner) invokes specialists during the dev-loop.

---

## How to Invoke

Use the **Task tool** with `subagent_type="general-purpose"` to spawn specialists.

**Benefits**:
- Real-time visibility into specialist progress
- Automatic tool access (Read, Edit, Write, Bash, Grep, Glob)
- Resume capability via agent ID
- No orphaned processes

---

## Building the Specialist Prompt

Read and concatenate these inputs:

| Input | Source | Required |
|-------|--------|----------|
| Specialist definition | `.claude/agents/{specialist}.md` | Yes |
| Accumulated knowledge | `docs/specialist-knowledge/{specialist}/*.md` | If exists |
| Matched principles | `docs/principles/*.md` (matched by task keywords) | Yes |
| Task description | From user/orchestrator | Yes |
| Findings to address | From previous iteration | If iteration 2+ |

### Prompt Structure

```markdown
{Contents of specialist definition file}

## Principles

{Contents of each matched principle file}

## Accumulated Knowledge

{Contents of patterns.md, gotchas.md, integration.md if they exist}

## Task

{Task description - VERBATIM from orchestrator}

## Findings to Address

{Findings from code review - VERBATIM, or omit section if iteration 1}

## Your Responsibilities

1. Implement the task (or fix the findings)
2. Run verification (cargo check, test, clippy)
3. Create checkpoint at `{output_dir}/{specialist}.md`
4. Update `{output_dir}/main.md` with Implementation Summary

## Required Output Format

End your response with this exact structure:

---RESULT---
STATUS: SUCCESS or FAILURE
SUMMARY: Brief description of what was done
FILES_MODIFIED: Comma-separated list of files changed
TESTS_ADDED: Number of tests added (0 if none)
VERIFICATION: PASSED or FAILED (did verification pass?)
ERROR: Error message if FAILURE, or "none" if SUCCESS
---END---
```

---

## Example Task Invocation

```
Task tool call:
  description: "AC specialist: implement feature X"
  subagent_type: "general-purpose"
  prompt: "{assembled prompt from above}"
```

---

## Parsing Specialist Response

The Task tool returns:
- The specialist's full response (including `---RESULT---` block)
- An agent ID for resume capability

**Extract status**:
- Look for `STATUS: SUCCESS` or `STATUS: FAILURE` in the response
- If `FAILURE`, report error to user

**Save agent ID**:
- Record in `main.md` Loop State for potential resume

---

## Resuming Specialists

To resume a specialist (for fixing findings or reflection):

```
Task tool call:
  description: "Resume AC specialist: fix findings"
  subagent_type: "general-purpose"
  resume: "{agent_id}"
  prompt: "{findings or reflection instructions}"
```

The resumed specialist retains full context from previous invocation.

---

## Critical Constraints

**Do NOT**:
- Paraphrase or summarize the task - pass it VERBATIM
- Add implementation suggestions or design guidance
- Specify function names, patterns, or architecture
- Write code yourself - the specialist writes code

**DO**:
- Read and concatenate the files as specified
- Pass task and findings exactly as received
- Record agent ID for resume capability
- Parse the `---RESULT---` block for status
- Report results to user for approval before proceeding

---

## Checkpoint Verification

After specialist returns, verify:
1. `STATUS: SUCCESS` in the `---RESULT---` block
2. Checkpoint file exists at `{output_dir}/{specialist}.md`

If checkpoint is missing or status is FAILURE, report to user.
