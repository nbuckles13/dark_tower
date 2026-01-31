---
name: dev-loop-init
description: Initialize a new dev-loop with output directory and matched principles. First step when starting implementation work.
---

# Dev-Loop Initialize

Initialize a new dev-loop. This skill:
1. Creates the output directory structure
2. Matches the task to principle categories
3. Identifies the implementing specialist
4. Prepares the specialist prompt preview
5. Creates the initial main.md with Loop State

**This skill does NOT invoke the specialist.** That happens in `/dev-loop-implement`.

## Arguments

The task description is passed via `$ARGUMENTS`:

```
/dev-loop-init "task description" [specialist-name] [--plan]
/dev-loop-init --plan [specialist-name]
```

- **task description** (optional if `--plan`): The implementation task (passed VERBATIM to specialist)
- **specialist-name** (optional): Override auto-detected specialist
- **--plan** (optional): Route to planning phase before implementation

### Invocation Patterns

| Invocation | Behavior |
|------------|----------|
| `/dev-loop-init "task"` | Ready for `/dev-loop-implement` (standard flow) |
| `/dev-loop-init "task" --plan` | Sets objective, routes to planning first |
| `/dev-loop-init --plan` | No objective yet, requires `/dev-loop-plan` to define |
| `/dev-loop-init "task" specialist-name` | Standard flow with explicit specialist |
| `/dev-loop-init "task" specialist-name --plan` | Planning flow with explicit specialist |

## Instructions

### Step 1: Check for Existing Active Loops

Before creating a new loop, check for active loops:

1. List directories in `docs/dev-loop-outputs/` (excluding `_template`)
2. Check each `main.md` for `Current Step != complete`
3. If active loop found:

```
**Warning**: Active dev-loop already exists at:
  docs/dev-loop-outputs/{existing-dir}/

Current step: {step}

Options:
1. Complete or abandon the existing loop first
2. Continue anyway (will create a new parallel loop)

Which would you like to do?
```

Wait for user response before proceeding.

### Step 2: Generate Output Directory Name

Create a slug from the task description:

```
Date: YYYY-MM-DD (today)
Task slug: lowercase, spaces→hyphens, max 50 chars, alphanumeric+hyphens only
Directory: docs/dev-loop-outputs/YYYY-MM-DD-{task-slug}/
```

### Step 3: Identify the Implementing Specialist

If specialist not provided, determine from task keywords:

| Pattern | Specialist |
|---------|------------|
| `auth|jwt|token|oauth|credential|login` | auth-controller |
| `meeting|session|signaling|webrtc|participant` | meeting-controller |
| `media|video|audio|stream|codec|sfu` | media-handler |
| `api|endpoint|route|http|gateway` | global-controller |
| `database|migration|schema|sql|query` | database |
| `test|coverage|fuzz|e2e` | test |

If no pattern matches, ask the user which specialist to use.

### Step 4: Match Task to Principle Categories

Apply regex patterns to task description (case-insensitive):

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

**Rules**:
- Multiple patterns can match → union of categories
- Limit to 3-4 categories max (attention budget)
- Always include `errors` for production code

**Principle files**: `docs/principles/{category}.md`

### Step 5: Check for Specialist Knowledge Files

Check if knowledge files exist at `docs/specialist-knowledge/{specialist}/`:
- `patterns.md`
- `gotchas.md`
- `integration.md`

Note which files exist for the implement step.

### Step 6: Create Output Directory and main.md

Create the directory:

```bash
mkdir -p docs/dev-loop-outputs/YYYY-MM-DD-{task-slug}
```

Determine initial state based on `--plan` flag:
- If `--plan` used: `Current Step = planning`
- Otherwise: `Current Step = init`

Create `main.md` from template with Loop State initialized:

```markdown
# Dev-Loop Output: {Task Title}

**Date**: YYYY-MM-DD
**Task**: {task description - VERBATIM, or "To be defined during planning" if --plan only}
**Branch**: `{current git branch}`
**Duration**: ~0m (in progress)

---

## Loop State (Internal)

<!-- This section is maintained by dev-loop skills for state recovery. -->

| Field | Value |
|-------|-------|
| Implementing Agent | `pending` |
| Implementing Specialist | `{specialist-name}` |
| Current Step | `{init or planning}` |
| Iteration | `1` |
| Security Reviewer | `pending` |
| Test Reviewer | `pending` |
| Code Reviewer | `pending` |
| DRY Reviewer | `pending` |

---

## Task Overview

### Objective
{task description - VERBATIM, or "To be defined during planning" if --plan only}

### Scope
- **Service(s)**: {inferred from specialist}
- **Schema**: TBD
- **Cross-cutting**: TBD

### Debate Decision
TBD - To be determined during implementation

---

## Matched Principles

The following principle categories were matched:
{list of matched categories with file paths}

---

## Pre-Work

TBD

---

{Rest of template sections with TBD placeholders}
```

### Step 7: Preview the Specialist Prompt

Show the user what will be passed to the specialist:

```
**Specialist Prompt Preview**

Specialist: {specialist-name}
Definition: .claude/agents/{specialist-name}.md

Principles to inject:
- docs/principles/{category1}.md
- docs/principles/{category2}.md
- ...

Knowledge files to inject:
- docs/specialist-knowledge/{specialist}/patterns.md (if exists)
- docs/specialist-knowledge/{specialist}/gotchas.md (if exists)
- docs/specialist-knowledge/{specialist}/integration.md (if exists)

Task (VERBATIM):
> {task description exactly as provided}

Output directory:
> docs/dev-loop-outputs/YYYY-MM-DD-{task-slug}/
```

### Step 8: Report Completion

#### If standard flow (no --plan):

```
**Dev-Loop Initialized**

Output directory: docs/dev-loop-outputs/YYYY-MM-DD-{task-slug}/
Specialist: {specialist-name}
Matched principles: {count} categories
Knowledge files: {count} files (or "none - will bootstrap on first reflection")

**Next step**: Run `/dev-loop-implement` to spawn the specialist
```

#### If planning flow (--plan used):

```
**Dev-Loop Initialized (Planning Mode)**

Output directory: docs/dev-loop-outputs/YYYY-MM-DD-{task-slug}/
Specialist: {specialist-name}
Matched principles: {count} categories
Knowledge files: {count} files (or "none - will bootstrap on first reflection")
Mode: Planning first

**Next step**: Run `/dev-loop-plan` to explore and propose approach
```

## Critical Constraints

- **VERBATIM task description**: Never paraphrase, summarize, or modify the task description
- **No implementation**: This skill prepares but does not implement
- **User approval point**: After showing the preview, user explicitly runs `/dev-loop-implement`

---

**Next step**: Run `/dev-loop-plan` (if planning mode) or `/dev-loop-implement` (if standard mode)
