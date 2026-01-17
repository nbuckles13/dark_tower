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
