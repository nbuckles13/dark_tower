# Specialist Knowledge Audit: 2026-01-16

## Trigger

After several dev-loops, specialist knowledge files had accumulated ~317 entries across 7 specialists in just 5 days. Analysis revealed:
- ~40% genuinely useful patterns and gotchas
- ~60% task-specific notes, universal best practices, or redundant entries
- Accumulation rate was unsustainable (~60 entries/day)
- Some files approaching the ~100 line guideline

See ADR-0017 for the specialist knowledge architecture design.

---

## Process

### Step 1: Add Curation Criteria

Updated `.claude/workflows/development-loop/step-reflection.md` with:
- **Curation Criteria** - 4 questions to ask before adding entries
- **Pruning Check** - 4 categories of entries to remove
- **Valid Reflection Outcomes** - Explicit permission for "no changes" reflections

### Step 2: Run Specialist Audits

Ran each specialist sequentially (to avoid API concurrency issues) with an audit prompt containing:
- New curation criteria
- New pruning criteria
- Instructions to be "aggressive about pruning"
- Request for summary statistics

**Specialists audited**: auth-controller, code-reviewer, security, test, global-controller, infrastructure, dry-reviewer

---

## Results

| Specialist | Before | After | Reduction |
|------------|--------|-------|-----------|
| auth-controller | 45 | 29 | 36% |
| code-reviewer | 57 | 34 | 40% |
| security | 58 | 27 | 53% |
| test | 57 | 41 | 28% |
| global-controller | 51 | 22 | 57% |
| infrastructure | 28 | 10 | 64% |
| dry-reviewer | 21 | 13 | 38% |
| **Total** | **~317** | **176** | **45%** |

**Git stats**: 22 files changed, 221 insertions(+), 2,430 deletions(-)

### What Was Removed

1. **Universal best practices** - Arrange-Act-Assert, DRY principle, use iterators, hash passwords
2. **Generic Rust/Axum patterns** - Standard error handling, state management documented in official docs
3. **Over-specific implementation details** - Descriptions of what code does without reusable insight
4. **Redundant entries** - Same pattern documented in multiple specialists' files
5. **Stale file references** - Paths that no longer exist or have moved

### What Was Fixed

1. **DRY-reviewer format compliance** - Added proper `Added:` dates and `Related files:` to all entries
2. **File path corrections** - Updated `repository/` → `repositories/`, `service/` → `services/`, etc.
3. **Line number removal** - Stripped `:72-84` style line references that become stale quickly
4. **Tech debt registry corrections** - TD-1 and TD-2 now reference actual existing files

---

## Lessons Learned

### What Worked Well

1. **Clear curation criteria** - The 4 questions gave agents a concrete decision framework
2. **Sequential execution** - Avoided API concurrency errors seen in previous parallel agent operations
3. **"Be aggressive" instruction** - Encouraged meaningful pruning vs. timid editing
4. **Summary statistics** - Before/after counts made impact visible and reviewable
5. **Format fixing** - Caught dry-reviewer's missing dates/file references

### What We'd Do Differently

1. **Inject specialist definitions** - Agents lacked domain context to make informed decisions. Should have included `.claude/agents/{specialist}.md` in the audit prompt so the agent understood what knowledge is relevant to that specialist's responsibilities.

2. **Verify file references upfront** - Could have provided a list of valid paths to help agents identify stale references.

3. **Consider injecting relevant ADRs** - For specialists with ADR-heavy domains (auth-controller, security), the ADRs provide important context for what knowledge is truly project-specific.

---

## Quality Assessment

Post-audit spot-check of remaining entries:
- **~80% clearly valuable** - Security patterns with CVE references, project-specific test harnesses, non-obvious gotchas
- **~20% borderline** - Patterns that document Dark Tower's approach to common problems (arguably useful, arguably standard)

The borderline entries aren't harmful - they just might not get referenced. Future reflections with the new curation criteria will naturally refine further.

---

## Commit

`04297c5` - Add knowledge curation criteria and audit specialist files
