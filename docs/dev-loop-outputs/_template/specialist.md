# {Specialist} Checkpoint - {Task Title}

<!--
This file is written by the specialist as they work.
It enables recovery if the session is interrupted.
Filename should match the specialist: e.g., global-controller.md, security.md, test.md, code-reviewer.md
-->

## Prompt Received

<!--
Record the prompt/instructions you received from the step-runner.
This enables auditing of what step-runners are telling specialists.
Copy the task description and any findings verbatim.
-->

**Task**: {copy task description here}

**Findings to address** (if any):
{copy findings verbatim, or "None - initial implementation"}

---

## Working Notes

### Patterns Discovered
<!-- What approaches worked well? What patterns did you follow or establish? -->

{Document patterns as you discover them}

### Gotchas Encountered
<!-- What mistakes did you catch? What was tricky? What would you warn others about? -->

{Document gotchas as you encounter them}

### Key Decisions
<!-- What choices did you make and why? Include alternatives considered. -->

{Document decisions as you make them}

### Observations
<!-- For reviewers: What did you notice during review? What informed your verdict? -->
<!-- For implementing specialists: Notable things about the codebase or task -->

{Document observations during work}

---

## Status

- **Step completed**: {implementation | review | reflection}
- **Verdict** (reviewers only): {APPROVED | FINDINGS | pending}
- **Last updated**: {ISO timestamp, e.g., 2026-01-14T15:30:00Z}

---

## Reflection Summary

<!-- Filled in during reflection step -->

### What I Learned
{Key takeaways from this task}

### Knowledge Updates Made
{List of updates to docs/specialist-knowledge/{specialist}/*.md, or "None"}

---

<!--
RESTORE CONTEXT: If this checkpoint is used for restore, the orchestrator will include:
1. The main.md Loop State (current step, iteration)
2. This checkpoint file's Working Notes
3. The task context from main.md
4. Instruction to continue from the current step
-->
