# owner: client
Clear all outstanding TypeScript/JavaScript dependency advisories so `pnpm audit
--audit-level=high` (forced, DEVLOOP_AUDIT_FORCE_RUN=1) reports zero unsuppressed
findings at or above the threshold. This task was auto-appended by the story
runner because the forced Layer-6 TS audit went red at story close.

PREFER A REAL FIX OVER SUPPRESSION. In order of preference:
1. Bump the vulnerable package to a patched version. For a direct dep, raise the
   range in the owning `packages/<pkg>/package.json` and `pnpm install`.
2. For a TRANSITIVE dep (advisory path shows `... > intermediate > vulnerable`),
   add or raise a `pnpm.overrides` entry in the root `package.json` forcing the
   patched version tree-wide, or bump the intermediate dep to a release that
   pulls the fixed version — whichever yields the smaller, cleaner lockfile delta.
3. SUPPRESS ONLY as a last resort, and ONLY when the advisory is genuinely
   unreachable in what we ship (e.g. a dev-tooling-only path such as a linter's
   transitive dep that never enters a client bundle).

SUPPRESSION IS GOVERNED — it is NOT the easy way out:
- pnpm has NO native ignore file. The ONLY sanctioned channel is
  `audit-suppressions.toml` at the repo root (ADR-0033 §11); it generates
  `.pnpm-audit-ignore.json`, which the audit wrapper's post-hoc filter reads.
  NEVER hand-edit the generated file, NEVER pass `--ignore` to pnpm audit (the
  wrapper blocks it), NEVER dismiss a Dependabot alert. See
  `docs/contributor/audit-suppressions.md`.
- A new entry MUST carry: id (GHSA-…), ecosystem `js`, one-line reason, an
  exposure analysis (why it is unreachable in shipped code — name the path, e.g.
  "dev linter transitive, tree-shaken / never bundled"), `expires` (ISO date,
  default 90 days), and a ticket. The always-run suppressions-check hard-fails on
  missing fields or a past-due date.
- After editing the manifest, regenerate derived files:
  `scripts/audit-suppressions-check.sh --fix`, and commit BOTH the manifest and
  the regenerated `.pnpm-audit-ignore.json`.
- Suppression policy is SECURITY's domain. If you add or extend any suppression,
  call it out EXPLICITLY to the security reviewer in your plan and at Gate 3 —
  they must scrutinize the exposure analysis. Do not let a suppression slip
  through as an incidental diff.

Acceptance: `DEVLOOP_AUDIT_FORCE_RUN=1 scripts/lang/ts/audit.sh` emits STATUS=OK
(zero unsuppressed advisories); `pnpm install` succeeds and `pnpm -w lint` /
svelte-check still pass (the lint toolchain may be what changed); any suppression
added is justified, expiring, and security-reviewed; `docs/TODO.md` §Dependency
Vulnerabilities (pnpm audit) / §Supply Chain updated. No cluster behavior
(env_tests false); Layers 1–6 gate the change.
