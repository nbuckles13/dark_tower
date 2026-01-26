---
name: dev-loop-reflect
description: Resume specialists sequentially to capture learnings in knowledge files. Run after /dev-loop-review approves.
disable-model-invocation: true
---

# Dev-Loop Reflect

After code review is approved, resume each specialist to reflect on learnings. Specialists update their knowledge files in `docs/specialist-knowledge/{specialist}/`.

**CRITICAL**: Specialists must be resumed **sequentially**, not in parallel. Parallel resume operations cause API errors due to tool use concurrency issues.

## Arguments

```
/dev-loop-reflect [output-dir]
```

- **output-dir** (optional): Override auto-detected output directory

## Instructions

### Step 1: Locate Active Dev-Loop

If output-dir not provided, auto-detect:

1. List directories in `docs/dev-loop-outputs/` (excluding `_template`)
2. Filter to `Current Step` = `reflection`
3. If exactly one: use it
4. If multiple: ask user which one
5. If none: error - "No dev-loop ready for reflection. Run `/dev-loop-review` first."

### Step 2: Get Agent IDs from Loop State

Read from `main.md` Loop State:

- Implementing Agent: `{agent_id}`
- Security Reviewer: `{agent_id}`
- Test Reviewer: `{agent_id}`
- Code Reviewer: `{agent_id}`
- DRY Reviewer: `{agent_id}`

### Step 3: Build Reflection Prompt

```markdown
## Reflection Phase

The code review is complete with no blocking findings. Now reflect on learnings from this implementation.

### Your Knowledge Files

Location: `docs/specialist-knowledge/{specialist}/`
- `patterns.md` - What worked well
- `gotchas.md` - What to avoid
- `integration.md` - Cross-service notes

### Curation Criteria

Before adding a new entry, ask:
1. **Would a fresh specialist benefit?** Would someone unfamiliar find this useful?
2. **Is this reusable or task-specific?** Patterns should generalize.
3. **Is this project-specific?** Universal best practices don't need entries.
4. **Does an existing entry cover this?** Update existing rather than duplicate.

If the answer to #1 or #2 is "no", don't add the entry.

### Pruning Check

Review existing entries for:
1. **Dead references**: File paths that no longer exist
2. **Superseded patterns**: Replaced by better approaches
3. **Over-specific entries**: Won't help future tasks
4. **Redundant entries**: Covered elsewhere

Remove or update stale entries. Pruning is valuable.

### Your Task

1. Read your knowledge files (create if they don't exist)
2. Add/update/remove entries based on this implementation
3. Each entry should have:
   - **Added**: {date}
   - **Related files**: {paths}
   - 2-4 sentence description
4. Update your checkpoint file with reflection summary
5. Return summary of changes

### Valid Outcomes

- **Added N entries**: New patterns/gotchas discovered
- **Updated N entries**: Existing entries refined
- **Pruned N entries**: Stale entries removed
- **No changes**: Existing knowledge was sufficient

"No changes" is valid. Don't add entries just to add them.

---

## Expected Return Format

```
knowledge_changes:
  added: N
  updated: N
  pruned: N
files_modified: [list of knowledge files touched]
summary: {2-3 sentence reflection}
```
```

### Step 4: Resume Specialists SEQUENTIALLY

**CRITICAL**: Resume ONE at a time, waiting for completion before the next.

**Order**:
1. Implementing specialist (first - has most context)
2. Security reviewer
3. Test reviewer
4. Code reviewer
5. DRY reviewer

For each specialist:

#### Attempt Resume

Use Task tool with `resume` parameter:

```
Task tool parameters:
- subagent_type: "general-purpose"
- resume: "{agent_id}"
- prompt: {reflection prompt}
```

#### If Resume Fails

Fall back to checkpoint injection:

1. Read the specialist's checkpoint file: `{output_dir}/{specialist}.md`
2. Invoke fresh agent with `general-purpose` subagent_type, model `Opus`, and checkpoint context:

```markdown
# Context Recovery for {Specialist}

You are continuing a dev-loop that was interrupted. Here's your previous context:

## Your Previous Working Notes

{paste checkpoint file content}

## Current Task

{reflection prompt}
```

#### Wait for Completion

Wait for the Task tool to return before proceeding to the next specialist.

### Step 5: Track Reflection Results

After each specialist completes, record:

| Specialist | Changes | Summary |
|------------|---------|---------|
| {name} | Added: N, Updated: N, Pruned: N | {summary} |

### Step 6: Update main.md

Add "Lessons Learned" section summarizing reflections:

```markdown
## Lessons Learned

### From {Implementing Specialist}
{reflection summary}

### From Security Review
{reflection summary}

### From Test Review
{reflection summary}

### From Code Review
{reflection summary}

### From DRY Review
{reflection summary}
```

### Step 7: Report Results

Update Loop State:

| Field | Value |
|-------|-------|
| Current Step | `complete` |

Report:

```
**Reflection Complete**

All specialists have reflected on learnings:

| Specialist | Added | Updated | Pruned |
|------------|-------|---------|--------|
| {implementing} | N | N | N |
| Security | N | N | N |
| Test | N | N | N |
| Code Reviewer | N | N | N |
| DRY Reviewer | N | N | N |

Knowledge files updated:
- docs/specialist-knowledge/{specialist}/patterns.md
- docs/specialist-knowledge/{specialist}/gotchas.md
- ...

**Next step**: Run `/dev-loop-complete`
```

## Critical Constraints

- **SEQUENTIAL resumes**: Never resume multiple specialists in parallel
- **Resume order**: Implementing specialist first, then reviewers
- **Checkpoint fallback**: If resume fails, use checkpoint injection (don't retry same ID)
- **No forcing entries**: "No changes" is a valid reflection outcome

## Knowledge File Structure

Location: `docs/specialist-knowledge/{specialist}/`

### patterns.md

```markdown
# {Specialist} Patterns

## Pattern Name
**Added**: YYYY-MM-DD
**Related files**: `path/to/file.rs`

Description of what works well (2-4 sentences).
```

### gotchas.md

```markdown
# {Specialist} Gotchas

## Gotcha Name
**Added**: YYYY-MM-DD
**Related files**: `path/to/file.rs`

Description of what to avoid (2-4 sentences).
```

### integration.md

```markdown
# {Specialist} Integration Notes

## Integration Topic
**Added**: YYYY-MM-DD
**Related files**: `path/to/file.rs`

Cross-service considerations (2-4 sentences).
```

---

**Next step**: Run `/dev-loop-complete`
