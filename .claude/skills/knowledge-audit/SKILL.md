---
name: knowledge-audit
description: Audit specialist knowledge files against the actual codebase. Run periodically or after major refactors to keep knowledge accurate.
---

# Knowledge Audit

Verify and prune specialist knowledge files against the actual codebase. The primary goal is **accuracy and conciseness** — remove stale entries, fix incorrect claims, and only rarely add new entries.

## Quality Bar

An entry belongs in specialist knowledge ONLY if it meets ALL of these:

1. **Non-obvious**: A developer reading the code wouldn't easily discover this
2. **Learned from experience**: It came from a real mistake, surprise, or debugging session
3. **Project-specific**: Not generic best practices or things clippy/linters catch
4. **Actionable**: It changes what a specialist would do in a future task

**Examples of good entries**: "ControllerMetrics vs ActorMetrics — only ActorMetrics emits to Prometheus, ControllerMetrics is for GC heartbeats only. This caused dashboard confusion."

**Examples of bad entries**: "All services use Kubernetes Deployments with rolling update strategy" (just read the YAML), "Use Result<T,E> for fallible operations" (generic Rust), "JWKS endpoint returns public keys" (obvious from code).

**If you can learn it by reading the code, don't add it.**

## When to Use

- **Quarterly** — Regular maintenance cadence
- **After major refactors** — Code changes may invalidate knowledge
- **Approaching limits** — When any knowledge file approaches ~400 lines
- **Post-migration** — After workflow or architecture changes

## Arguments

```
/knowledge-audit                          # Audit all specialists
/knowledge-audit --specialists=security,test  # Audit specific specialists
```

## Instructions

### Step 1: Pre-Audit Metrics

Run baseline counts for the audit report:

```bash
# Entry counts per specialist
for dir in docs/specialist-knowledge/*/; do
  [ -d "$dir" ] || continue
  [[ "$(basename $dir)" == "audits" ]] && continue
  echo -n "$(basename $dir): "
  grep -c "^## " "$dir"*.md 2>/dev/null | awk -F: '{sum+=$2} END {print sum}'
done

# Line counts
wc -l docs/specialist-knowledge/*/*.md | grep -v audits | sort -n
```

Record these numbers for the audit report.

### Step 2: Determine Scope

If `--specialists` provided, audit only those. Otherwise, audit all specialists with knowledge directories under `docs/specialist-knowledge/`.

### Step 3: Compose Specialist Prompts

For each specialist, compose a prompt with:

1. **Specialist identity**: `.claude/agent-teams/specialists/{name}.md`
2. **Their knowledge files**: ALL `.md` files from `docs/specialist-knowledge/{name}/`
3. **Audit instructions** (see template below)

**Audit prompt template**:

```
You are the {Name} specialist for Dark Tower. Audit your knowledge files against the actual codebase.

## Your Identity

{contents of .claude/agent-teams/specialists/{name}.md}

## Your Current Knowledge Files

{contents of ALL files in docs/specialist-knowledge/{name}/}

## Quality Bar

An entry belongs here ONLY if ALL of these are true:
1. **Non-obvious**: A developer reading the code wouldn't easily discover this
2. **Learned from experience**: It came from a real mistake, surprise, or debugging session
3. **Project-specific**: Not generic best practices or things clippy/linters catch
4. **Actionable**: It changes what you would do in a future task

If you can learn it by reading the code, it does NOT belong here.

## Audit Instructions

### Phase 1: Read the Codebase

Read the actual code, ADRs, and scripts relevant to your domain:
- Source code in your domain (see your specialist definition for file paths)
- Relevant ADRs in docs/decisions/
- Guard scripts in scripts/guards/ (if security/observability/test)
- Recent dev-loop outputs that involved your domain

### Phase 2: Verify and Prune (PRIMARY TASK)

For each existing entry:
1. Is the claim still accurate? (Check the actual code)
2. Do file references still exist and point to the right thing?
3. Does it meet the quality bar above? If not, DELETE it.
4. Is it duplicating what an ADR already says? If so, DELETE it.
5. Can a developer learn this just by reading the code? If so, DELETE it.

**Bias toward deletion.** An audit that removes 30% of entries and adds nothing is a successful audit.

### Phase 3: Add ONLY If Warranted

Only add a new entry if you found something that:
- Would have saved significant debugging time if documented
- Represents a non-obvious interaction between components
- Captures a decision rationale that isn't in any ADR

**Do NOT add entries just because you read code and can describe what it does.** That's documentation, not knowledge.

**If your knowledge files are empty or near-empty**: That's fine. It means this specialist hasn't been through enough implementation cycles yet. Knowledge accumulates through `/dev-loop` reflection, not through audits. Do not bulk-populate empty files.

### Phase 4: Edit Your Files

Make the actual edits to your knowledge files in docs/specialist-knowledge/{name}/:
- **Delete** entries that don't meet the quality bar
- **Update** entries with stale file references or inaccurate claims
- **Add** entries only if they meet the quality bar (expect 0-2 additions per audit)

### Format Requirements

Each entry must have:
- `**Added**: YYYY-MM-DD` (use today's date for new entries)
- `**Related files**: path/to/files`
- 2-4 sentence description (concise — not paragraphs)
- `---` separator between entries

### Output

After auditing, report:
- Entries deleted (with reason for each)
- Entries updated (what changed)
- Entries added (must justify against quality bar)
- Entries kept unchanged (count)

A good audit deletes more than it adds.
```

### Step 4: Spawn Audit Team

Spawn all specialists in parallel as Task agents. Each specialist only edits their own knowledge directory — no conflicts.

Lead monitors for completion and collects summaries.

### Step 5: Post-Audit Verification

After all specialists complete:

**Check file references exist**:
```bash
grep -rh "Related files" docs/specialist-knowledge/ | grep -v audits | \
  grep -oE '`[^`]+`' | tr -d '`' | \
  while read f; do [ -e "$f" ] || echo "Missing: $f"; done
```

**Check line counts stayed reasonable** (no file should exceed 400 lines):
```bash
wc -l docs/specialist-knowledge/*/*.md | grep -v audits | sort -n
```

**Review git diff** — deletions should outnumber or match insertions:
```bash
git diff --stat docs/specialist-knowledge/
```

### Step 6: Document Results

Create audit report at `docs/specialist-knowledge/audits/YYYY-MM-DD-{description}.md`:

```markdown
# Specialist Knowledge Audit: YYYY-MM-DD

## Trigger

{Why this audit was run}

## Results

| Specialist | Before (entries) | After (entries) | Deleted | Updated | Added |
|------------|-----------------|-----------------|---------|---------|-------|
| {name} | {n} | {n} | {n} | {n} | {n} |

### Summary of Changes

{High-level themes across all specialists}

### Notable Deletions

{What was pruned and why}

### Notable Additions (if any)

{What was added and why it meets the quality bar}
```

### Step 7: Report to User

```
**Knowledge Audit Complete**

Specialists audited: {count}
Entries before: {total}
Entries after: {total}
Net change: {+/- n}

Notable changes:
- {specialist}: {one-line summary}
- ...

Report: docs/specialist-knowledge/audits/YYYY-MM-DD-{slug}.md
```

## Limits

| Phase | Limit | Action |
|-------|-------|--------|
| Per specialist | 30 minutes | Proceed with partial audit |
| Total audit | 2 hours | Complete what's done, document partial |
| Post-verification | 15 minutes | Proceed without |

## Output

- **Updated knowledge files**: `docs/specialist-knowledge/{name}/*.md`
- **Audit report**: `docs/specialist-knowledge/audits/YYYY-MM-DD-{slug}.md`

## Related

- ADR-0017 — Specialist knowledge architecture
- `.claude/agent-teams/specialists/` — Specialist definitions
- `/dev-loop` reflection phase — How knowledge gets captured during implementation
