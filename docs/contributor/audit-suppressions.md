# Audit advisory suppressions

How to suppress a dependency security advisory in Dark Tower, when to suppress vs
fix, and how to renew a suppression before it expires. The machinery is task #47
(ADR-0033 §3 amendment + §11).

## The single source of truth

`audit-suppressions.toml` (repo root) is the **ONLY sanctioned place** to suppress a
security advisory (ADR-0033 §11). Everything else is generated from it or prohibited:

- `.cargo/audit.toml` and `.pnpm-audit-ignore.json` are **GENERATED** from the
  manifest — never hand-edit them.
- Ad-hoc CLI flags (`cargo audit --ignore=…`, `pnpm audit --ignore=…`) are **not**
  a suppression channel — the wrappers deliberately refuse flag pass-through.
- Dismissing a Dependabot or code-scanning alert in the GitHub UI is **prohibited**
  as a suppression mechanism (see "Dependabot" below).

A suppression only takes effect when it lands as a reviewed PR editing the manifest.

## When to suppress vs fix

**Fix** (preferred) when a patched version exists: bump the dependency, regenerate
the lockfile, and let the advisory clear. Dependabot will often open the bump PR for
you.

**Suppress** only when *all* of:
- there is no fixed upstream version (or the fix is blocked on an upstream problem), and
- you have an in-tree exposure analysis showing the vulnerable code is not reachable
  in our runtime (e.g. build-time-only), and
- security has reviewed and approved the justification.

If you can fix it, fix it. Suppression is for genuinely-unfixable, demonstrably-
unreachable advisories — and even then it expires (below).

## How to add a suppression

1. Add an entry to `audit-suppressions.toml`:
   ```toml
   [[suppression]]
   id        = "RUSTSEC-YYYY-NNNN"   # or GHSA-… for js
   ecosystem = "rust"                # rust | js
   expires   = "YYYY-MM-DD"          # default: today + 90 days
   ticket    = "docs/TODO.md#…"      # tracking pointer
   reason    = "one-line summary; full rationale in a #-comment above the entry"
   ```
   Put the **full exposure analysis** in a `#`-comment block immediately above the
   entry (it is mirrored verbatim into the generated `.cargo/audit.toml`).
2. Regenerate the derived files:
   ```
   scripts/audit-suppressions-check.sh --fix
   ```
3. **Commit BOTH** the manifest AND the regenerated derived files
   (`.cargo/audit.toml`, `.pnpm-audit-ignore.json`) in the same PR. The always-run
   sync-check (Layer 3) FAILs if they drift.
4. Open the PR for security review (the suppression *reason*/*expires* is security-
   owned, ADR-0033 §11).

## Default duration: 90 days

New suppressions default to a **90-day** `expires`. The always-run
`scripts/audit-suppressions-check.sh` (Layer-3 guard, every devloop + CI) hard-fails
any entry **strictly past** `expires`: the entry is valid *through* its `expires`
date, and CI goes red **the day after** **by design** (e.g. `expires = 2026-09-05`
is green through 2026-09-05 and fails with `REASON=suppression-past-due` from
2026-09-06; see `__date_check`). That is the discipline backstop: a suppression
must be periodically re-justified, not left to rot.

## Renewal workflow (when a suppression expires, or to renew proactively)

CI red with `REASON=suppression-past-due` is the renewal signal (the first signal —
there is no warn-ahead window in this round, so renew proactively per the cadence
below). Renewing is the **sanctioned** remediation — it is NOT an ad-hoc incident-time
silencing:

1. **Re-verify the justification.** For RUSTSEC-2023-0071, re-run the build-time-only
   invariant: `cargo tree -p rsa --invert` — confirm the runtime tree is still empty.
   If the invariant no longer holds, do NOT renew — fix or re-justify with a fresh
   exposure analysis.
2. **Edit `expires`** in `audit-suppressions.toml` to a new date (default +90 days),
   updating the `reason`/comment if the analysis changed.
3. **Regenerate:** `scripts/audit-suppressions-check.sh --fix`.
4. **Commit BOTH** the manifest and the regenerated derived files.
5. **Open a PR** for security review — security reviews the renewed justification.

What you must NOT do: extend an `expires` date as part of live incident triage just to
make CI green. That is prohibited and escalates to security (see the runbook §6.3
exception note). Renewal flows through the tracked manifest and normal PR review.

## Fail-closed posture

- A malformed manifest (missing field, bad date, duplicate id) → the check FAILs
  (`REASON=suppression-malformed`), never silently "0 suppressions".
- Derived files drifting from the manifest → FAIL (`REASON=suppression-drift`).
- The dep-change-gated scan runs the audit on any doubt (indeterminate diff) — it only
  skips on a proven no-dependency-change.

## Dependabot

`.github/dependabot.yml` opens weekly version-bump PRs (cargo + npm/pnpm). Dependabot
is for **bump PRs only**:

- **Do NOT** add an `ignore:` block to `dependabot.yml`, and **do NOT** dismiss a
  Dependabot or code-scanning alert in the GitHub UI, as a way to suppress an advisory.
  Those are out-of-tree, unreviewed suppression surfaces that would fragment the single
  source of truth. The manifest (reviewed PR) is the only sanctioned path. The
  `dependabot.yml` `ignore:` rule is MECHANICALLY ENFORCED: the always-run
  `scripts/audit-suppressions-check.sh` FAILs with `REASON=dependabot-ignore-present`
  if a non-empty `ignore:` block appears — so it sticks for future PRs, not just today.
- It is **desirable** for Dependabot to open a bump PR for a currently-suppressed dep
  (e.g. rsa) — that is remediation progress toward removing the suppression, not a
  conflict with it.

**Manual enablement step:** committing `dependabot.yml` enables Dependabot
*version-updates*. Dependabot *security-updates* (auto-PR when an advisory is published)
require a repository **Settings → Code security → Dependabot security updates** toggle
that a maintainer must flip — it cannot be committed. Flip it to get advisory-driven
bump PRs in addition to the weekly scheduled scan.

## See also

- `audit-suppressions.toml` — the manifest.
- `scripts/audit-suppressions-check.sh` — the always-run check (`--fix` regenerates).
- `docs/runbooks/devloop-validation.md` §6.3 — failure triage for the suppression checks.
- ADR-0033 §3 (amendment) + §11 — the validation-pipeline + audit-policy contract.
