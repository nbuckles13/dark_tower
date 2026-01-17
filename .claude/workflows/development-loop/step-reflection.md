# Step: Reflection

This file is read when entering the `reflection` step of the dev-loop.

---

## Purpose

After code review is clean, all specialists reflect on learnings. This builds the knowledge base while context is fresh.

---

## Critical: Resume, Don't Re-invoke

Specialists must be **resumed** (not invoked fresh) to preserve context. This ensures reflection captures genuine learnings, not summaries.

If resume fails, use checkpoint injection. See `session-restore.md` for the fallback pattern.

---

## Resume Sequentially, Not in Parallel

**Resume one specialist at a time**, waiting for each to complete before resuming the next.

Parallel resume operations cause `API Error: 400 due to tool use concurrency issues`. The backend cannot handle multiple simultaneous agent resume calls.

**Order**: Resume implementing specialist first, then each reviewer.

**If resume fails**: Don't retry the same agent ID. Instead, spawn a fresh agent with checkpoint injection per `session-restore.md` "Resume Fallback Pattern" section.

---

## Who Reflects

| Specialist | Agent ID Source |
|------------|-----------------|
| Implementing specialist | Saved from implementation step |
| All reviewers | Saved from code review step |

All agent IDs should be stored in the Loop State table.

---

## Curation Criteria

Before adding a new entry, ask yourself:

1. **Would a fresh specialist benefit?** Would someone unfamiliar with this task find this useful on a similar future task?
2. **Is this reusable or task-specific?** Patterns should generalize beyond this one implementation.
3. **Is this project-specific?** Universal best practices (Arrange-Act-Assert, DRY) don't need entries.
4. **Does an existing entry cover this?** Update existing entries rather than creating duplicates.

If the answer to #1 or #2 is "no", don't add the entry.

---

## Pruning Check

During each reflection, review existing entries for:

1. **Dead references**: File paths that no longer exist or have moved significantly
2. **Superseded patterns**: Approaches replaced by better ones
3. **Over-specific entries**: Implementation details that won't help future tasks
4. **Redundant entries**: Knowledge now covered by other entries or project docs

Remove or update entries that match these criteria. Pruning is as valuable as adding.

---

## Valid Reflection Outcomes

Not every task produces new knowledge. Valid reflection outcomes include:

- **Added N entries**: New patterns/gotchas discovered
- **Updated N entries**: Existing entries refined or corrected
- **Pruned N entries**: Stale or redundant entries removed
- **No changes**: Existing knowledge was sufficient; nothing new learned

"No changes" is a valid outcome. Don't add entries just to add them.

---

## What Specialists Do

1. Review knowledge in `docs/specialist-knowledge/{specialist}/`
2. Add/update/remove entries based on learnings
3. Append reflection summary to checkpoint file

**Knowledge file format**: See existing files in `docs/specialist-knowledge/*/` for examples.

**Guidelines**:
- ~100 lines per file
- Each entry has Added date + Related files
- Keep descriptions to 2-4 sentences

---

## Bootstrap Behavior

First-time reflection: Specialist creates `docs/specialist-knowledge/{specialist}/` with initial:
- `patterns.md`
- `gotchas.md`
- `integration.md`

---

## Reflection Prompt Template

```markdown
## Reflection Phase

The code review is complete with no blocking findings. Now reflect on learnings.

### Your Knowledge Files

Location: `docs/specialist-knowledge/{specialist}/`
- `patterns.md` - What worked well
- `gotchas.md` - What to avoid
- `integration.md` - Cross-service notes

### Your Task

1. Review your knowledge files
2. Add/update/remove entries based on this implementation
3. Each entry should have:
   - **Added**: {date}
   - **Related files**: {paths}
   - 2-4 sentence description
4. Write a brief reflection summary to your checkpoint file

### Learnings to Consider

{List specific patterns, gotchas, or integration notes from this task}
```

---

## Approval Flow

Knowledge file changes appear in git diff alongside implementation. User reviews and commits everything together.

---

## State Transition

**After all reflections complete**:
1. Run Pre-Completion Validation Checklist (see `output-documentation.md`)
2. Update Loop State to `complete`
3. Update Duration in output file header
