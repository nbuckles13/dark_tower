---
name: dev-loop-review
description: Spawn 4 code reviewers in parallel to review the implementation. Run after /dev-loop-validate passes.
disable-model-invocation: true
---

# Dev-Loop Review

Invoke 4 code review specialists in parallel:
1. Security Specialist
2. Test Specialist
3. Code Reviewer
4. DRY Reviewer

Each reviewer examines the implementation and provides a verdict.

## Arguments

```
/dev-loop-review [output-dir]
```

- **output-dir** (optional): Override auto-detected output directory

## Instructions

### Step 1: Locate Active Dev-Loop

If output-dir not provided, auto-detect:

1. List directories in `docs/dev-loop-outputs/` (excluding `_template`)
2. Filter to `Current Step` = `code_review`
3. If exactly one: use it
4. If multiple: ask user which one
5. If none: error - "No dev-loop ready for code review. Run `/dev-loop-validate` first."

### Step 2: Identify Files to Review

Get list of modified files from the implementation:

```bash
git diff --name-only HEAD~1
```

Or read from the "Files Modified" section of main.md.

### Step 3: Match to Principle Categories

Use the same principle matching as init step to ensure reviewers check the same principles the implementer was given.

### Step 4: Build Reviewer Prompts

For each reviewer, build a prompt with:

1. **Specialist definition**: `.claude/agents/{reviewer}.md`
2. **Accumulated knowledge** (if exists): `docs/specialist-knowledge/{reviewer}/*.md`
3. **Relevant principles**: From matched categories
4. **Files to review**: List of modified files
5. **Review instructions**: Specific focus areas

#### Prompt Template

```markdown
{specialist definition content}

---

## Accumulated Knowledge

{content of patterns.md, gotchas.md, integration.md if they exist}

---

## Principles to Check

{content of matched principle files}

---

## Code Review Task

Review the following files for this implementation task:

**Task**: {task description from main.md}

**Files to Review**:
{list of modified files}

---

## Your Responsibilities

1. Read each modified file
2. Check against your domain expertise and the principles above
3. Identify issues with severity (ALL except TECH_DEBT require fixes):
   - **BLOCKER**: Fundamental flaw, requires redesign
   - **CRITICAL**: Security or correctness issue, must fix
   - **MAJOR**: Significant issue, must fix
   - **MINOR**: Small issue, must fix (NOT optional)
   - **TECH_DEBT**: (DRY/Code Reviewer only) Non-blocking, document for later
4. Write checkpoint to `{output_dir}/{reviewer-name}.md` with:
   - Observations
   - Findings by severity
   - Verdict: APPROVED | REQUEST_CHANGES | BLOCKED
5. Return structured output

---

## Verdict Rules (STRICT - follow exactly)

- **APPROVED**: No findings, OR only TECH_DEBT findings. Nothing else qualifies.
- **REQUEST_CHANGES**: ANY finding of BLOCKER, CRITICAL, MAJOR, or MINOR severity. Even one MINOR = REQUEST_CHANGES.
- **BLOCKED**: Fundamental architectural issues requiring complete redesign.

**IMPORTANT**: MINOR findings ARE blocking. Do NOT mark APPROVED if you have ANY non-TECH_DEBT findings.

---

## Expected Return Format

```
verdict: APPROVED | REQUEST_CHANGES | BLOCKED
finding_count:
  blocker: N
  critical: N
  major: N
  minor: N
  tech_debt: N
checkpoint_exists: true | false
summary: {2-3 sentence summary}
```
```

### Step 5: Invoke All 4 Reviewers in Parallel

Use the **Task tool** with `general-purpose` subagent_type and model `Opus` for each reviewer.

**CRITICAL**: Invoke all 4 in a single message with multiple Task tool calls for parallel execution.

```
Reviewers to invoke:
1. security - Security vulnerabilities, crypto, auth
2. test - Test coverage, edge cases, test quality
3. code-reviewer - Code quality, Rust idioms, maintainability
4. dry-reviewer - Cross-service duplication
```

### Step 6: Capture Agent IDs

After all Task tools return, capture each agent ID.

Update `main.md` Loop State:

| Field | Value |
|-------|-------|
| Security Reviewer | `{agent_id}` |
| Test Reviewer | `{agent_id}` |
| Code Reviewer | `{agent_id}` |
| DRY Reviewer | `{agent_id}` |

### Step 7: Verify Checkpoints Exist

Check that each reviewer created their checkpoint:

```bash
test -f {output_dir}/security.md && echo "security exists"
test -f {output_dir}/test.md && echo "test exists"
test -f {output_dir}/code-reviewer.md && echo "code-reviewer exists"
test -f {output_dir}/dry-reviewer.md && echo "dry-reviewer exists"
```

### Step 8: Synthesize Verdicts

Collect verdicts from all reviewers:

| Reviewer | Verdict | Findings |
|----------|---------|----------|
| Security | {verdict} | {count by severity} |
| Test | {verdict} | {count by severity} |
| Code Reviewer | {verdict} | {count by severity} |
| DRY Reviewer | {verdict} | {count by severity} |

**Overall verdict rules**:
- **APPROVED**: All reviewers APPROVED
- **REQUEST_CHANGES**: Any reviewer REQUEST_CHANGES (and no BLOCKED)
- **BLOCKED**: Any reviewer BLOCKED

### Step 9: Update main.md Code Review Section

Add or update the "Code Review Results" section:

```markdown
## Code Review Results

### Security Specialist
**Verdict**: {verdict}
{list of findings}

### Test Specialist
**Verdict**: {verdict}
{list of findings}

### Code Quality Reviewer
**Verdict**: {verdict}
{list of findings}

### DRY Reviewer
**Verdict**: {verdict}
**Blocking findings**: {list or "None"}
**Tech debt findings**: {list or "None"}
```

### Step 10: Report Results

#### If APPROVED (All Reviewers)

Update Loop State:

| Field | Value |
|-------|-------|
| Current Step | `reflection` |

Report:

```
**Code Review Approved**

All 4 reviewers approved:
- Security: APPROVED ✓
- Test: APPROVED ✓
- Code Reviewer: APPROVED ✓
- DRY Reviewer: APPROVED ✓

**Next step**: Run `/dev-loop-reflect`
```

#### If REQUEST_CHANGES or BLOCKED

Update Loop State:

| Field | Value |
|-------|-------|
| Current Step | `fix` |

Collect all findings into a list:

```
**Code Review Requires Changes**

Verdicts:
- Security: {verdict}
- Test: {verdict}
- Code Reviewer: {verdict}
- DRY Reviewer: {verdict}

**Findings to Address**:

1. [{severity}] {reviewer}: {finding description} - {file:line}
2. [{severity}] {reviewer}: {finding description} - {file:line}
...

**Tech Debt** (non-blocking, documented):
- {tech debt items}

**Next step**: Run `/dev-loop-fix`
```

## Critical Constraints

- **Parallel invocation**: All 4 reviewers MUST be invoked in a single message
- **Capture all agent IDs**: Need them for reflection step
- **TECH_DEBT does not block**: Only document tech debt findings, don't require fixes
- **Checkpoint required**: Each reviewer must create checkpoint file

## Reviewer Definitions

| Reviewer | File | Focus |
|----------|------|-------|
| Security | `.claude/agents/security.md` | Vulnerabilities, crypto, auth |
| Test | `.claude/agents/test.md` | Coverage, test quality |
| Code Reviewer | `.claude/agents/code-reviewer.md` | Quality, idioms, maintainability |
| DRY Reviewer | `.claude/agents/dry-reviewer.md` | Cross-service duplication |

---

**Next step**: Run `/dev-loop-reflect` (if approved) or `/dev-loop-fix` (if changes required)
