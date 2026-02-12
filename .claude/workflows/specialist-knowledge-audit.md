# Workflow: Specialist Knowledge Audit

Periodic audit of specialist knowledge files to maintain quality and sustainability.

---

## When to Run

- **Quarterly** - Regular maintenance cadence
- **Approaching limits** - When any knowledge file approaches ~100 lines
- **After major refactors** - Code changes may invalidate file references
- **Low signal complaints** - When specialists report noise in their knowledge files

---

## Pre-Audit Checklist

### 1. Check Entry Counts

```bash
for dir in docs/specialist-knowledge/*/; do
  [ -d "$dir" ] || continue
  [[ "$(basename $dir)" == "audits" ]] && continue
  echo -n "$(basename $dir): "
  grep -c "^## " "$dir"*.md 2>/dev/null | awk -F: '{sum+=$2} END {print sum}'
done
```

### 2. Check Line Counts

```bash
wc -l docs/specialist-knowledge/*/*.md | grep -v audits | sort -n
```

### 3. Identify Priority Specialists

Focus on specialists with:
- Highest entry counts
- Files approaching 100 lines
- Recent significant code changes in their domain

---

## Audit Process

### Run Specialists Sequentially

Run one specialist audit at a time to avoid API concurrency issues.

### Audit Prompt Template

```markdown
## Knowledge File Audit: {Specialist} Specialist

### Your Specialist Definition

{Inject full contents of .claude/agents/{specialist}.md}

### Curation Criteria

**For keeping entries:**
1. Would a fresh specialist benefit from this on a similar task?
2. Is this a reusable pattern or just how this specific code works?
3. Is this project-specific knowledge (not universal best practices)?

**For removing entries:**
1. File references that no longer exist or moved
2. Patterns superseded by better approaches
3. Over-specific implementation details that won't generalize
4. Entries redundant with other entries or project docs

### Your Task

1. Read your knowledge files in `docs/specialist-knowledge/{specialist}/`
2. For each entry, evaluate against the criteria above
3. Remove entries that don't meet curation criteria
4. Update entries with stale file references
5. Keep entries that are genuinely reusable

### Format Requirements

Each entry must have:
- `**Added**: YYYY-MM-DD`
- `**Related files**: path/to/files`
- 2-4 sentence description
- `---` separator between entries

### Output

After auditing, provide a summary:
- Entries kept (with brief justification for any borderline cases)
- Entries removed (with reason)
- Entries updated (what changed)

Be aggressive about pruning. It's better to have 10 high-quality entries than 30 mediocre ones.

Make the actual edits to the files.
```

### Critical: Inject Specialist Definition

The specialist definition (`.claude/agents/{specialist}.md`) gives the auditing agent context about:
- What domain the specialist owns
- What responsibilities they have
- What kinds of knowledge are relevant to their work

Without this, the agent makes generic "is this useful?" decisions rather than domain-informed curation.

---

## Post-Audit Verification

### 1. Check Entry Counts

```bash
grep -c "^## " docs/specialist-knowledge/*/patterns.md
```

### 2. Verify File References Exist

```bash
grep -h "Related files" docs/specialist-knowledge/*/*.md | \
  grep -oE '`[^`]+`' | tr -d '`' | \
  while read f; do [ -e "$f" ] || echo "Missing: $f"; done
```

### 3. Review Git Diff

```bash
git diff --stat docs/specialist-knowledge/
```

### 4. Spot-Check Quality

Read a sampling of remaining entries to verify they meet the curation bar.

---

## Document Results

After each audit, create a result file:

**Location**: `docs/specialist-knowledge/audits/YYYY-MM-DD-{description}.md`

**Template**:

```markdown
# Specialist Knowledge Audit: YYYY-MM-DD

## Trigger

{Why this audit was run - quarterly, approaching limits, post-refactor, etc.}

## Process

{How the audit was conducted - which specialists, any variations from standard process}

## Results

| Specialist | Before | After | Reduction |
|------------|--------|-------|-----------|
| ... | ... | ... | ... |
| **Total** | **...** | **...** | **...%** |

### What Was Removed

{Categories of entries that were pruned}

### What Was Fixed

{Format issues, stale references, etc.}

## Lessons Learned

{What worked, what to improve for next time}

## Commit

`{hash}` - {commit message summary}
```

---

## Related Documents

- `.claude/skills/dev-loop/SKILL.md` - Dev-loop workflow (reflection phase captures knowledge)
- `.claude/skills/dev-loop-restore/SKILL.md` - Session restore patterns
- ADR-0017 - Specialist knowledge architecture
- `docs/specialist-knowledge/audits/` - Previous audit results
