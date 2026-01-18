# Specialist Invocation (for Step-Runners)

This file describes how step-runners invoke specialists during the dev-loop.

---

## How to Invoke

Use the **Task tool** with `general-purpose` subagent_type.

Build the prompt by reading and concatenating these inputs (provided by orchestrator):

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
```

---

## Critical Constraints

**Do NOT**:
- Paraphrase or summarize the task - pass it VERBATIM
- Add implementation suggestions or design guidance
- Specify function names, patterns, or architecture
- Write code yourself - the specialist writes code

**DO**:
- Read and concatenate the files as specified
- Pass task and findings exactly as received from orchestrator
- Verify checkpoint exists after specialist returns

---

## Checkpoint Verification

After specialist returns, verify:
1. Checkpoint file exists at `{output_dir}/{specialist}.md`
2. Checkpoint has "Prompt Received" section filled in (for audit trail)

If checkpoint is missing, report failure to orchestrator.

---

## Specialist Checkpoint Template

Specialists should record what prompt they received. See `docs/dev-loop-outputs/_template/specialist.md` for the template with "Prompt Received" section.
