---
name: dev-loop-complete
description: Mark dev-loop as complete, run final validation, summarize results. Final step of the dev-loop.
disable-model-invocation: true
---

# Dev-Loop Complete

Finalize a dev-loop by:
1. Running the pre-completion validation checklist
2. Calculating total duration
3. Updating Loop State to `complete`
4. Summarizing the work done
5. Suggesting commit

## Arguments

```
/dev-loop-complete [output-dir]
```

- **output-dir** (optional): Override auto-detected output directory

## Instructions

### Step 1: Locate Active Dev-Loop

If output-dir not provided, auto-detect:

1. Run `./scripts/workflow/dev-loop-status.sh --active-only`
2. Filter output to loops with `Current Step` in (`reflection`, `complete` if re-running)
3. If exactly one: use it
4. If multiple: ask user which one
5. If none: error - "No dev-loop ready for completion."

### Step 2: Run Pre-Completion Validation Checklist

Check ALL of the following in `main.md`:

| Check | How to Verify | If Failed |
|-------|---------------|-----------|
| No TBD/placeholder content | Search for "TBD", "TODO", "PLACEHOLDER" | Fill in or flag |
| Implementation Summary filled | Has actual tables/lists, not just headers | Flag |
| Files Modified populated | Has file list | Extract from git diff |
| Verification steps documented | Each layer has PASS/FAIL status | Flag |
| Issues Encountered documented | Has issues or explicit "None" | Flag |
| Lessons Learned populated | Has 2+ concrete learnings | Flag |
| Tech Debt documented | Has table or explicit "None" | Flag |

#### Validation Commands

```bash
# Check for placeholder content
grep -E "(TBD|TODO|PLACEHOLDER)" {output_dir}/main.md

# Check Implementation Summary has content
grep -A 10 "## Implementation Summary" {output_dir}/main.md | grep -E "^\|"
```

### Step 3: Report Validation Issues

If any checks fail, report but allow user to proceed:

```
**Pre-Completion Validation**

Issues found:
- [ ] TBD content remains in: {sections}
- [ ] Implementation Summary is empty
- [ ] ...

Would you like to:
1. Fix these issues before completing
2. Complete anyway (issues will be noted)
```

Wait for user input.

### Step 4: Calculate Duration

Get the start time from `main.md` header (`**Date**:` and `**Start Time**:`) and calculate elapsed time from start to current time.

Example calculation:
- Start: 2026-02-02 14:30
- End: 2026-02-02 16:45
- Duration: 2h 15m

Update the Duration field:

```markdown
**Duration**: ~{X}h {Y}m
```

For durations under 1 hour, use minutes only: `~{X}m`

### Step 5: Update Loop State to Complete

Update `main.md` Loop State:

| Field | Value |
|-------|-------|
| Current Step | `complete` |

### Step 6: Generate Summary

Create a summary of the completed work:

```
**Dev-Loop Complete**

**Task**: {task description}
**Duration**: ~{X}m
**Specialist**: {implementing specialist}
**Iterations**: {iteration count}

**Files Changed**:
{summary from Files Modified section}

**Verification**: All 7 layers passed ✓

**Code Review**:
- Security: APPROVED ✓
- Test: APPROVED ✓
- Code Reviewer: APPROVED ✓
- DRY Reviewer: APPROVED ✓

**Knowledge Updated**:
- {list of knowledge files modified}

**Tech Debt Tracked** (if any):
- {list from Tech Debt section}
```

### Step 7: Suggest Commit

```
**Ready to Commit**

The following files are ready to commit:

**Implementation**:
{list of modified source files}

**Documentation**:
- {output_dir}/main.md
- {output_dir}/{specialist checkpoints}

**Knowledge**:
- docs/specialist-knowledge/{specialist}/*.md (if modified)

Suggested commit message:

```
{task description}

- Implemented by {specialist} specialist
- All 7 verification layers passed
- Code review approved by 4 specialists
- Knowledge files updated

See: {output_dir}/main.md

Co-Authored-By: Claude Opus 4.5 <noreply@anthropic.com>
```

To commit, run your usual git commands or use the /commit skill.
```

## Critical Constraints

- **Validation first**: Always run pre-completion validation
- **Don't auto-commit**: Suggest commit but let user decide
- **Mark complete**: Loop State must be updated to `complete`
- **Calculate duration**: Update the Duration field

## Pre-Completion Checklist (Full)

From `output-documentation.md`:

| Check | Verification |
|-------|--------------|
| Correct output directory | Directory name matches task slug |
| No TBD/placeholder content | Search for TBD, TODO, PLACEHOLDER |
| Implementation Summary filled | Has actual content |
| Files Modified populated | Has file list with changes |
| Verification steps documented | Each of 7 layers has status |
| Issues Encountered documented | Has issues or "None" |
| Lessons Learned populated | Has 2+ learnings |
| Tech Debt documented | Has table or "None" |
| Duration filled | Has actual duration |

---

**Dev-loop complete!** Ready to commit when you're satisfied with the changes.
