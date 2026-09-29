---
name: absorb-devloop
description: Bring a devloop clone's commits into the current branch, then triage the Claude memories that devloop saved
---

# Absorb Devloop

```
/absorb-devloop <slug>
```

`<slug>` is the devloop slug passed to `devloop.sh`; its clone is `../worktrees/<slug>/`.

## Step 1: Plan

```bash
./infra/devloop/absorb-devloop.sh --dry-run <slug>
```

Report the plan (fast-forward or cherry-pick, the commits, any skipped as already present). If the clone has uncommitted work, stop and tell the user.

## Step 2: Absorb

```bash
./infra/devloop/absorb-devloop.sh <slug>
```

On a conflict (exit 3), resolve, `git add` the files, and rerun the same command; it finishes the paused commit and absorbs the rest.

- `docs/specialist-knowledge/*/INDEX.md` and `docs/devloop-outputs/**`: take the incoming side (`git checkout --theirs`).
- Everything else: read both sides and merge. If a resolution needs a judgment call beyond combining both sides, ask the user.
- If the pre-commit hook refuses the finished commit (e.g. a Gate-2 verdict for a devloop-completion commit), stop and ask the user. Bypassing the hook is their decision.

Report the result with `git log --oneline` for the absorbed range.

## Step 3: Triage the devloop's memories

Claude inside the devloop container saves memories there; `devloop.sh` copies them to `~/.cache/devloop/memory-inbox/<slug>/` after each session and before removing the container. If the container still exists, harvest it first:

```bash
source infra/devloop/memory-harvest.sh && harvest_container_memory devloop-<slug>-dev <slug>
```

If the inbox has no memory files (other than `MEMORY.md`), say so and stop.

For each memory file, pick one outcome:

| Outcome | When |
|---|---|
| **Drop** | Already stated in CLAUDE.md, a skill, an ADR, a runbook, a specialist INDEX or an existing host memory (name where); only meaningful inside that devloop's session; or wrong. |
| **Host memory** | Durable and not derivable from the repo: the user's preferences or feedback on how to work, external references. Merge into an existing memory when one covers the topic. |
| **Repo doc** | A rule or fact every devloop should follow that belongs in CLAUDE.md, a skill, a specialist INDEX or a runbook. Name the file and the edit. |

A memory that restates existing repo guidance usually means that guidance was missed. Drop it, and note whether the existing text needs to be clearer or more prominent.

Present one table — file, one-line summary, recommended outcome, reason or target — and ask the user to approve or change it. Then:

1. Write approved host memories (your auto-memory directory and its `MEMORY.md` index).
2. Make approved repo edits and commit them separately.
3. Move the triaged files to `~/.cache/devloop/memory-inbox/.triaged/<slug>-<YYYY-MM-DD>/` (kept as a record).
