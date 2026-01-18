# Output Documentation

This file covers the output file format, validation checklist, and documentation responsibilities.

---

## Purpose

The output file serves as:
- **Proof-of-work** - Evidence that the specialist ran all required steps
- **Audit trail** - Record of the development process
- **Historical reference** - Patterns in what works/doesn't

---

## Shared Responsibility

| Section | Written By | When |
|---------|------------|------|
| Task Overview | Specialist | During implementation |
| Implementation Summary | Specialist | During implementation |
| Files Modified | Specialist | During implementation |
| Verification Results (7 layers) | Specialist | After each verification run |
| Issues Encountered | Specialist | As issues arise |
| Reflection | Each specialist | During reflection step |
| Code Review Results | Orchestrator | After code review |
| Final Validation | Orchestrator | Before presenting to user |

---

## Output Location

```
docs/dev-loop-outputs/YYYY-MM-DD-{task-slug}/
├── main.md              # Main output (orchestrator owns Loop State)
├── {specialist}.md      # Implementing specialist checkpoint
└── {reviewer}.md        # One checkpoint per code reviewer
```

---

## Templates

- Main output: `docs/dev-loop-outputs/_template/main.md`
- Specialist checkpoint: `docs/dev-loop-outputs/_template/specialist.md`

---

## Pre-Completion Validation Checklist

**⚠️ CRITICAL**: Before advancing Loop State to `complete`, verify ALL of the following:

| Check | How to Verify | If Failed |
|-------|---------------|-----------|
| Correct output directory | Directory name matches task slug from conversation | Rename/consolidate files |
| No TBD/placeholder content | Search for "TBD", "TODO", "PLACEHOLDER" in main.md | Fill in or resume specialist |
| Implementation Summary filled | Section has actual tables/lists, not just headers | Fill in from implementation |
| Files Modified populated | Has file list with line counts | Extract from git diff |
| Verification steps documented | Each of 7 layers has status (PASS/FAIL/SKIPPED) | Re-run verification |
| Issues Encountered documented | Either has issues or explicit "None" | Add issues from conversation |
| Lessons Learned populated | Has 2+ concrete learnings | Extract from conversation |
| Tech Debt documented | Has table or explicit "None" | Check DRY reviewer findings |
| Duration filled | Has actual duration, not "TBD" | Calculate from timestamps |

---

## Validation Script

```bash
./scripts/workflow/verify-dev-loop.sh --output-dir docs/dev-loop-outputs/YYYY-MM-DD-{task-slug} --verbose
```

Or manually:
```bash
# Check for placeholder content
grep -E "(TBD|TODO|PLACEHOLDER)" "$OUTPUT_DIR/main.md" && echo "FAIL: Placeholder content found"

# Check Implementation Summary has content (not just header)
awk '/## Implementation Summary/{found=1} found && /^\|/{has_content=1} /^##/ && found && !/## Implementation Summary/{exit} END{if(!has_content) print "FAIL: Implementation Summary empty"}' "$OUTPUT_DIR/main.md"
```

---

## Why This Matters

The Loop State table tracks orchestration state, but the output file is proof-of-work. A complete Loop State with incomplete documentation means the specialist did the work but didn't document it - this loses valuable context for future reference.

---

## Orchestrator Appends

After validation, orchestrator appends:
- Code review results (from all reviewers)
- Final validation timestamp
- Any orchestrator-level observations

---

## Loop State Section Format

The Loop State table is maintained in main.md for state recovery:

```markdown
## Loop State (Internal)

| Field | Value |
|-------|-------|
| Implementing Specialist | `global-controller` |
| Current Step | code_review |
| Iteration | 2 |
| Security Reviewer | `def456` |
| Test Reviewer | `ghi789` |
| Code Reviewer | `jkl012` |
| DRY Reviewer | `mno345` |
```

---

## Categories Shorthand

Use these categories when describing what areas code touches (in Implementation Summary, Code Review, etc.):

| Category | Key Concerns |
|----------|--------------|
| `crypto` | secrets, keys, hashing, encryption |
| `jwt` | token validation, claims, expiry |
| `logging` | no secrets in logs, structured format |
| `queries` | parameterized SQL, no injection |
| `errors` | no panics, proper types |
| `input` | validation, limits, sanitization |
| `testing` | test ownership, three tiers, determinism |
| `concurrency` | actor pattern, message passing |
| `api-design` | URL versioning, deprecation |
| `observability` | privacy-by-default, metrics, spans |
