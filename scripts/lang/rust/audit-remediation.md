# owner: infrastructure
Clear all outstanding Rust dependency advisories so `cargo audit` (forced,
DEVLOOP_AUDIT_FORCE_RUN=1) reports zero unsuppressed findings at or above the
configured threshold. This task was auto-appended by the story runner because
the forced Layer-6 Rust audit went red at story close.

PREFER A REAL FIX OVER SUPPRESSION. In order of preference:
1. Bump the vulnerable crate to a patched version — usually a `cargo update -p
   <crate> --precise <ver>` plus the resulting `Cargo.lock` change; for a direct
   dep, bump the version in the owning crate's `Cargo.toml`. Rebuild + test.
2. If the advisory is reached only transitively and the direct dep has no patched
   release, try a `[patch]` or a minimal-version bump of the intermediate dep.
3. SUPPRESS ONLY as a last resort, and ONLY when the advisory is genuinely
   unreachable in what we ship (e.g. a build-time-only or unused-feature path).

SUPPRESSION IS GOVERNED — it is NOT the easy way out:
- The ONLY sanctioned channel is `audit-suppressions.toml` at the repo root
  (ADR-0033 §11). NEVER hand-edit the generated `.cargo/audit.toml`, NEVER pass
  `--ignore` to cargo audit (the wrapper blocks it), NEVER dismiss a Dependabot
  alert. See `docs/contributor/audit-suppressions.md`.
- A new entry MUST carry: id, ecosystem `rust`, one-line reason, an exposure
  analysis with a fail-closed verify command (e.g. `cargo tree -p <crate>
  --invert` showing only build-time consumers), `expires` (ISO date, default 90
  days), and a ticket. The always-run suppressions-check hard-fails on missing
  fields or a past-due date.
- After editing the manifest, regenerate derived files:
  `scripts/audit-suppressions-check.sh --fix`, and commit BOTH the manifest and
  the regenerated `.cargo/audit.toml`.
- Suppression policy is SECURITY's domain. If you add or extend any suppression,
  call it out EXPLICITLY to the security reviewer in your plan and at Gate 3 —
  they must scrutinize the exposure analysis. Do not let a suppression slip
  through as an incidental diff.

Acceptance: `DEVLOOP_AUDIT_FORCE_RUN=1 scripts/lang/rust/audit.sh` emits STATUS=OK
(zero unsuppressed advisories); any suppression added is justified, expiring, and
security-reviewed; `docs/TODO.md` §Dependency Vulnerabilities (cargo audit) /
§Supply Chain updated to reflect what changed. No cluster behavior (env_tests
false); Layers 1–6 gate the change.
