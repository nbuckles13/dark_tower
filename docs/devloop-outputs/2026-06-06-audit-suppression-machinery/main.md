# Devloop Output: Audit-suppression machinery + conditional audit + date enforcement

**Date**: 2026-06-06
**Task**: Build a single-source-of-truth audit-suppression manifest with date enforcement,
generated derived configs, conditional (dep-change-gated) audit scanning, an always-run
suppressions hygiene check, contributor docs, and a TODO mirror. (Story task #47.)
**Specialist**: infrastructure (paired with security)
**Mode**: Agent Teams (v2) — full, `--paired-with=security`
**Branch**: `feature/browser-client-join-task-47`
**Duration**: TBD

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `63ca0fbc9cd5c62bbd34a717de042681fc410c8a` |
| Branch | `feature/browser-client-join-task-47` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer@devloop-audit-suppression` |
| Implementing Specialist | `infrastructure` |
| Paired Specialist | `security` (expanded role: active co-implementer of suppression policy + Gate 2/3 reviewer) |
| Iteration | `1` |
| Security (paired) | `security@devloop-audit-suppression` |
| Test | `test@devloop-audit-suppression` |
| Observability | `observability@devloop-audit-suppression` |
| Code Quality | `code-reviewer@devloop-audit-suppression` |
| DRY | `dry-reviewer@devloop-audit-suppression` |
| Operations | `operations@devloop-audit-suppression` |
| Semantic Guard | `semantic-guard@devloop-audit-suppression` |

### Gate 1 — Plan Confirmations

| Reviewer | Plan Status |
|----------|-------------|
| Security (paired) | confirmed — §3 co-sign UNCONDITIONAL (drift-catcher in #47). Policy fully locked: expiry 2026-09-01, verbatim reason, §D.1.2 split, placement, FAIL-branch coverage, runbook/L365 verbatim, 4 REASON tokens, force-run-only env, cross-boundary. Gate-3: L365 wording + acceptance tests + 2 hunk-ACKs |
| Test | confirmed (T1/T2/T3 + edge matrix §I; layer3 wire-in accepted; glob-runner = accepted spin-out TODO) |
| Observability | confirmed (SKIPPED-NO-DIFF + SUPPRESSED= line; WARN deferred) |
| Code Quality | confirmed (Plan-confirmed to Lead; 3 pinned reqs resolved + D-b/keep-ALWAYS_RUN + inline parse + §D.1.2 split + `.pnpm-audit-ignore.json` shared `read_pnpm_ignore_ids` helper, `.cargo/audit.toml` single-caller) |
| DRY | confirmed (2 watch-points pinned: §D over-trigger gate via helpers only; §B single parse_suppressions) |
| Operations | confirmed (§3 BLESSED joint w/ security; §F=WEEKLY in #47; no ci.yml step; OC1–OC5) — **re-confirmed expanded BOTH scope**: issue dedup+auto-close+least-priv token, runbook §6.3 two-surfaces, dependabot limit/groups |
| Semantic Guard | confirmed (fail-closed: non-zero exit + no-downgrade bar) |

**GATE 1 PASSED** (2026-06-06). All 7 confirmed; classification-sanity guard `STATUS=OK`; drift-catcher
= BOTH (user ruling); security §F.2 bless contingent on (a1) no-`ignore:` + (a2) dependabot.yml
point-of-temptation comment + (b3) contributor-doc UI-dismissal prohibition — folded as build criteria.
"Plan approved" sent to implementer.

### Gate 1 Design Requirements (Lead-pinned from code-reviewer's review)
1. **Classification upgrade**: `scripts/lang/{rust,ts}/audit.sh` rows → **Minor-judgment (security)**
   (suppressed-advisory filtering is security-policy content). Resolve via `Approved-Cross-Boundary:
   security` hunk-ACK trailers on the filtering + always-run-gate hunks; paired security co-signs.
2. **STATUS enum (N/A vs SKIPPED-NO-DIFF)**: N/A out-ranks OK in worst-status aggregation; converge
   with observability (STATUS-contract owner) on the exit-0, non-dominating signal; document choice +
   rationale. Intent (cheap, non-masking skip) governs over the literal "N/A" in the task text.
3. **ADR-0033 §3 amendment note (mandatory, explicit)**: conditional scan reverses §3's *rejected*
   "audit only on lockfile touch" alternative — land a real amendment note naming the compensating
   control (always-run date-check + suppression expiry), blessed by operations + security.

---

## Task Overview

### Objective
Audit-noise follow-up to the PR #58 force-merge. Replace the unconditional "scan every advisory
every run" posture with: (a) a single-source-of-truth suppression manifest, (b) cheap always-run
date/hygiene enforcement, (c) dep-change-gated scanning. Six deliverables:

1. `audit-suppressions.toml` (repo root) — `[[suppression]]` tables: `id`, `ecosystem` (rust|js),
   `reason`, `expires` (ISO date), `ticket`. Seed RUSTSEC-2023-0071 (expires **2026-09-01**,
   ticket `docs/TODO.md`).
2. `scripts/audit-suppressions-check.sh` — three always-run checks: **date-check** (hard-fail
   past-due: days-past + IDs + "either re-suppress with new date or fix"), **sync-check** (manifest
   IDs ↔ generated `.cargo/audit.toml` + `.pnpm-audit-ignore.json`), **quality-check** (reason +
   ticket non-empty). `--fix` regenerates derived files when the manifest is newer.
3. Conditional audit — `scripts/lang/{rust,ts}/audit.sh` exit `STATUS=N/A REASON=no-dep-changes`
   (N/A per brief; **CORRECTED to `SKIPPED-NO-DIFF` at Gate 1 — see Design Req #2 + §D STATUS-enum
   decision**; N/A would dominate worst-status, SKIPPED-NO-DIFF is the right non-masking signal)
   when no dep manifests are in the changed-file set; suppressed advisories filter out of the
   failure list.
4. Always-run the suppressions-check in CI (`.github/workflows/ci.yml`) AND in devloops.
5. `docs/contributor/audit-suppressions.md` (default 90d, when-to-suppress-vs-fix, renewal).
6. `docs/TODO.md` "Suppressed Advisories" section mirroring the manifest.

### KEY DESIGN TENSIONS (for Gate 1 — flagged by Lead)
- **ADR-0033 §3 always-run amendment**: today `cargo audit` / `pnpm audit` are classified
  **always-run** (ADR-0033 §3/§6 Layer 6 matrix; the wrappers' header comments cite this and
  deliberately refuse CLI flag pass-through as a security finding). Task #47 makes the *scan*
  conditional and moves the always-run guarantee to the cheap *suppressions-check*. This is a real
  ADR-0033 §3 reclassification — **operations + security (paired) must bless it at Gate 1**, and it
  likely needs an ADR-0033 §3 amendment note. Net posture must not weaken: the always-run
  date-check must fire even when no deps changed (that is the discipline backstop).
- **Expiry-date discrepancy**: task arg says seed RUSTSEC-2023-0071 `expires 2026-09-01`; the
  security-authored policy block in `docs/TODO.md` (Devloop I, Cluster C) says 90-day sunset
  **2026-08-08**. Security (paired) adjudicates which governs and whether the rich
  build-time-only rationale (the `cargo tree -p rsa --invert` invariant) is preserved in the
  manifest `reason` and/or the generated `.cargo/audit.toml` comments.
- **Derived-file formats**: `.cargo/audit.toml` is cargo-audit-native (`[advisories] ignore=[...]`);
  pnpm has no clean native ignore file, so `.pnpm-audit-ignore.json` is a custom filter the
  ts wrapper applies post-hoc. Confirm both formats at Gate 1.

### Scope
- **Service(s)**: none (repo-root tooling, CI, docs). No Rust/TS service code.
- **Schema**: No.
- **Cross-cutting**: Yes — validation pipeline (ADR-0033) + CI + security audit policy (ADR-0033 §11).

### Debate Decision
NOT NEEDED — task is explicitly user-specified ("per case discussion") with security paired in.
The ADR-0033 §3 amendment is in-scope design, adjudicated at Gate 1 by operations + security, not a
separate `/debate`.

### Out of scope (per task brief + #48)
- Clearing fixable advisories / version bumps (that is task #48).
- RUSTSEC-2023-0071 remediation beyond seeding the suppression (also #48 boundary).

---

## Cross-Boundary Classification

Implementer is **infrastructure** (owns `scripts/`, CI, repo-root tooling). The suppression
*policy content* (which advisories are suppressed, the rationale, expiry) is **security's domain**
per ADR-0033 §11 — hence `--paired-with=security` (security co-implements the policy entries).
The conditional-audit change touches the always-run security gate, so security co-signs that too.

One discrete row per touched path (the `validate-cross-boundary-scope` guard matches per-path —
no compound/bundled rows, no rows for paths not in the diff). Verified against `git status` at
Gate 2 attempt 1.

| Path | Classification | Owner (if not mine) |
|------|----------------|---------------------|
| `audit-suppressions.toml` | Not mine, Minor-judgment (policy content) | security (paired) |
| `.cargo/audit.toml` (generated) | Not mine, Minor-judgment (security ignore-list, ADR-0033 §11) | security (paired) |
| `.pnpm-audit-ignore.json` (generated) | Not mine, Minor-judgment | security (paired) |
| `scripts/audit-suppressions-check.sh` | Mine | — |
| `scripts/audit-suppressions-check.test.sh` | Mine | — |
| `scripts/lang/_audit_gate.sh` | Mine (machinery) — shared dep-gate + suppression-filter helper; security co-signs the gate/filter semantics | security (paired) |
| `scripts/lang/_audit_suppressions_lib.sh` | Mine (machinery) — shared pure `.pnpm-audit-ignore.json` parse locus (extracted at Gate 2 per @dry-reviewer: one parse, two fail-policies) | — |
| `scripts/lang/_audit_gate.test.sh` | Mine — hermetic tri-state gate matrix test (added Gate 2 per @test/Lead F2; wired into layer3.sh) | — |
| `scripts/lang/rust/audit.sh` | **Not mine, Minor-judgment (security)** — suppressed-advisory *filtering* + always-run-gate change is security-policy content (the "silence advisories at runtime" the header comment guards) | security (paired) |
| `scripts/lang/ts/audit.sh` | **Not mine, Minor-judgment (security)** — same as rust: filtering + gate change is security-policy content | security (paired) |
| `scripts/guards/simple/audit-suppressions.sh` (new Layer-3 guard) | Mine | — |
| `scripts/layer3.sh` (guard auto-discovered; self-test wire-in; CI-leak assertion) | Mine | — |
| `scripts/layer-all.sh` (CI-sentinel-leak runtime assertion, §J/C) | Mine | — |
| `scripts/lang/_test_helpers.sh` (`assert_status`/`assert_exit` additions) | Mine | — |
| `scripts/lang/_common.sh` (shared `assert_no_ci_sentinel_leak` — single-locus CI-leak assertion, extracted at Gate 2 per @dry-reviewer) | Mine | — |
| `.github/workflows/audit-scheduled.yml` (weekly scan + §F.1 deduped-issue surfacing) | Mine (infra) — `DEVLOOP_AUDIT_FORCE_RUN` gate-bypass semantics security co-signed | security (paired) |
| `.github/dependabot.yml` (cargo + npm/pnpm, weekly; NO `ignore:`; §F.2) | Mine (infra) — second-suppression-surface POLICY (manifest-is-SSOT, no-`ignore:`) is security's call | security (paired) |
| `docs/decisions/adr-0033-polyglot-validation-pipeline.md` (§3 amendment + rejected-alt back-ref) | Mine (infra) — the §3 amendment is ops+security co-signed (process ADR, not single-owner GSA) | — |
| `docs/contributor/audit-suppressions.md` | Mine | — |
| `docs/runbooks/devloop-validation.md` (§6.3 rows + L365 exception) | Mine (machinery) — L365 hunk is security-reviewed wording | security (paired) |
| `docs/TODO.md` | Mine (doc mirror) | — |

Note: `scripts/lang/audit.sh` (the dispatcher) is **NOT** touched — `DEVLOOP_AUDIT_FORCE_RUN` is read by
the wrappers via `_audit_gate.sh` through env inheritance, so no dispatcher edit was needed. And
`.github/workflows/ci.yml` is **NOT** touched (the no-explicit-ci.yml-step decision, operations q4).
Neither appears as a table row.

**Classification upgrade accepted (Gate 1, @code-reviewer):** the two `audit.sh` wrapper rows are
upgraded from "Mine (machinery)" to **Minor-judgment (security)**. Rationale: the suppression
*filtering* logic and the always-run→conditional gate change are security-policy content, not pure
plumbing — they decide *which advisories get silenced* and *when the scan runs*, both of which the
Wave-1 header comment explicitly flags as security's domain. No ESCALATE: I accept the upgrade.
**Commitment:** the filtering hunks and the always-run-gate hunks in `scripts/lang/{rust,ts}/audit.sh`
will carry `Approved-Cross-Boundary: security <reason>` hunk-ACK trailers, co-signed by @security
(paired) at Gate 2.

No GSA paths (ADR-0024 §6.4) are touched — audit-config files are not in the enumerated GSA list,
and there is no wire-format/auth-routing/schema/forensics coupling. Security ownership here is via
ADR-0033 §11 (audit policy), operationalized through the pairing, not GSA routing.

---

## Planning

**Author**: infrastructure (implementer), drafted with @security pairing on policy content.
**Status**: awaiting Gate 1 confirmations.

### A. Manifest design — `audit-suppressions.toml` (repo root)

Single source of truth. One `[[suppression]]` array-of-tables per advisory. **@security delivered
the final seed-ready entry (their msg 2026-06-06, decisions 1–4); it lands verbatim.** Fields:
`id`, `ecosystem` (`rust`|`js`), `expires` (ISO `YYYY-MM-DD`), `ticket` (non-empty), `reason`
(non-empty, **single-line** basic string).

Security's decision (A): **`expires = "2026-09-01"` GOVERNS** — the 90-day clock re-anchors to the
manifest-landing date; the stale 2026-08-08 must NOT appear as `expires` (I update the TODO.md
mirror 2026-08-08 → 2026-09-01 with a "(re-anchored to manifest-landing date)" note).

**Rationale placement (security decision 2, FINAL):** the rich load-bearing exposure analysis lives
in THREE places — (a) a **one-line `reason`** in the manifest (the summary, verbatim string below),
(b) the **full block as a `#`-comment ABOVE the entry** in the manifest, (c) **verbatim in the
generated `.cargo/audit.toml`** (mandatory — a bare `ignore=[...]` with no rationale at the point of
suppression is a security finding). docs/TODO.md Cluster C stays the long-form record. This is a
REVISION of security's earlier draft (which put the full block INTO `reason`): the final shape uses
a one-line `reason` + comment block.

**Parser simplification (welcome consequence):** because `reason` is now a single-line string and
the rich block is a `#`-comment (skipped by the parser, not a value), the pure-bash parser needs NO
multiline `"""` tokenizer — every parsed field is a flat one-per-line `key = "value"`, and `#` lines
are comments. This DROPS the awk-state-machine complexity I'd flagged earlier to @code-reviewer.
Cleaner parse, still zero-dep. (Heads-up sent to @code-reviewer + @dry-reviewer that the multiline
tokenizer is no longer needed.)

#### A.1 Final RUSTSEC-2023-0071 manifest entry (security-authored, seed-ready)

The one-line `reason` (verbatim from @security):

```toml
# RUSTSEC-2023-0071 — rsa 0.9.10 Marvin Attack (timing side-channel on RSA decryption).
#
# Exposure analysis (security, 2026-05-08; re-anchored to new manifest 2026-06-06):
#   - Pulled transitively by sqlx-macros-core compile-time query-validation
#     scaffolding, NOT by any runtime driver we select.
#   - Workspace sqlx features = ["runtime-tokio","postgres","uuid","chrono",
#     "migrate"]. No "mysql" feature; no runtime code path constructs rsa::*.
#   - INVARIANT (verify command of record): `cargo tree -p rsa --invert`
#     must show ONLY build-time consumers; no shipped binary links rsa runtime.
#   - Marvin requires attacker-observable RSA decrypt timing on a server we
#     operate. We don't ship rsa code, so there is no such surface.
#
# Upstream: no fix in rsa 0.9.x; constant-time RSA in Rust is an unsolved
# upstream problem (rust-lang/rsa#19). Waiting for a fix is waiting indefinitely.
# Expected to be the longest-lived entry here.
#
# FAIL-CLOSED: if `cargo tree -p rsa --invert` ever shows a runtime consumer,
# this suppression is void immediately — remove it, fix, or re-justify with a
# new exposure analysis. No grace period.
#
# Sunset: 90-day heavy re-justification at `expires` (2026-09-01) — re-run the
# invert command, confirm the runtime tree is still empty, re-affirm in-tree.
# ADR-0033 §12 14-day MTTR tripwire continues to apply independently.
[[suppression]]
id = "RUSTSEC-2023-0071"
ecosystem = "rust"
expires = "2026-09-01"
ticket = "docs/TODO.md#dependency-vulnerabilities"
reason = "rsa 0.9.10 Marvin timing side-channel; build-time-only via sqlx-macros (no mysql feature, no runtime rsa::*) — verify with `cargo tree -p rsa --invert`; fail-closed if a runtime consumer ever appears; no upstream fix. Full rationale in comment above + docs/TODO.md Cluster C."
```

The manifest file also carries a top-of-file header (single-source-of-truth notice + "DO NOT
hand-edit the derived files" pointer). Policy ownership: security (ADR-0033 §11); machinery:
infrastructure.

#### A.2 Derived-file generation requirements (security decision B)

- **`.cargo/audit.toml`** — cargo-audit native:
  ```toml
  [advisories]
  ignore = ["RUSTSEC-2023-0071"]
  ```
  cargo-audit's `audit.toml` carries no per-id rationale natively, so the generator **MUST emit the
  rationale as TOML comments around the ignore entry** — at minimum: the build-time-only invariant,
  the `cargo tree -p rsa --invert` command of record, the FAIL-CLOSED condition, and
  `expires = 2026-09-01`. A bare `ignore = [...]` with no rationale comment is a SECURITY FINDING
  (rationale must be readable at the point of suppression, not only in the source manifest).
- **`.pnpm-audit-ignore.json`** — empty suppression set today (the RUSTSEC entry is `rust`-only):
  `{ "ignore": [] }`.
- **Both** derived files carry a `# DO NOT EDIT — generated from audit-suppressions.toml` header
  (JSON: a `"_generated"` marker key, since JSON has no comments) so no one hand-edits them (that
  would be an ad-hoc out-of-band suppression path — closed by sync-check, see C3).

### B. `scripts/audit-suppressions-check.sh` — three ALWAYS-RUN checks

STATUS contract per ADR-0033 §6 (sourced `_common.sh`, `emit_status`). Final stdout line is a
single `STATUS=<OK|FAIL> REASON=<token>`. This is the discipline backstop: it runs regardless of
whether any dep manifest changed. **DISTINCT REASON tokens per failure mode (@security runbook
requirement, NOT generic `guards-failed`)** so on-call greps to the right §6.3 runbook row and each
expiry-fire is an auditable security event:

1. **date-check** (hard-fail past-due) → `REASON=suppression-past-due`: for each entry, compare
   `expires` to `NOW` (injectable via `AUDIT_SUPPRESSIONS_NOW`, default `date -u +%F` — see §I/T1
   determinism). **Semantic: FAIL only on strictly `expires < NOW`** — `expires == NOW` (expires
   today) is still VALID (not yet past); this boundary is stated in an in-code comment and tested
   both ways (T1). On a past-due entry, FAIL listing each past-due ID with days-past, ending with the
   literal guidance + renewal pointer (operations OC4). Full shape:
   `FAIL: RUSTSEC-2023-0071 expired N days ago (expires 2026-09-01). Either re-suppress with new date
   or fix the advisory. Renewal workflow: docs/contributor/audit-suppressions.md`.
2. **sync-check** → `REASON=suppression-drift`: the set of IDs in the manifest must exactly equal the
   set of IDs in the derived files — rust IDs ↔ `.cargo/audit.toml` `[advisories] ignore=[...]`; js
   IDs ↔ `.pnpm-audit-ignore.json`. Drift (manifest edited but `--fix` not run) → FAIL with the diff.
3. **quality-check** → `REASON=suppression-quality`: `reason` and `ticket` non-empty for every entry;
   `expires` matches `^\d{4}-\d{2}-\d{2}$`; `ecosystem` ∈ {rust, js}.
4. **parse-error** (manifest or derived file present-but-unparseable) → `REASON=suppression-malformed`
   (§D.1.2 fail-closed; distinct from "absent/empty" which is OK).
5. **override-without-valid-sentinel** (a test-injection env present but `DEVLOOP_TEST` != exactly
   `"1"` — absent OR wrong value) → `REASON=suppression-override-without-test-sentinel` (§J/B
   sentinel-gating: a bare override in a non-test environment trips the gate RED, never silently
   dropped; the EXACT-match removes the `DEVLOOP_TEST=0` truthy bypass).
6. **CI-sentinel-leak** (`GITHUB_ACTIONS=true` AND `DEVLOOP_TEST` set, any value) →
   `REASON=test-sentinel-set-in-ci` — a runtime assertion EARLY in `layer-all.sh`/`layer3.sh` (§J/C,
   operations) that hard-fails if the test sentinel leaks into the CI job env, which would otherwise
   silently let the production check honor overrides repo-wide.

(Token names are @security's/@operations'; tokens 1–5 map 1:1 to §6.3 runbook keyed-failure rows +
the consolidated failure-index. Token 6 lives at the `layer-all`/`layer3` boundary (not the check
script) but also gets a failure-index row — both 5 and 6 are security-relevant gate-integrity events,
greppable as their own failures. See §K.)

`--fix`: regenerates `.cargo/audit.toml` and `.pnpm-audit-ignore.json` from the manifest. Guarded
by "manifest is newer" (mtime) per the task brief, but `--fix` also unconditionally regenerates on
demand so a contributor can force a rewrite. Regeneration writes a generated-file header
(`# GENERATED — edit audit-suppressions.toml then run scripts/audit-suppressions-check.sh --fix`).

The three checks all run and accumulate; the script emits one aggregated FAIL if any check failed,
OK otherwise. shellcheck-clean.

**Single parse function (DRY — pinned with @dry-reviewer, watch-point 2):** all four consumers
(date-check, sync-check, quality-check, `--fix`) route through ONE internal
`parse_suppressions` function that reads `audit-suppressions.toml` once and emits normalized
pipe-delimited records `id|ecosystem|expires|ticket|reason` (one per `[[suppression]]`). NO
copy-pasted awk/grep passes over the TOML — four near-identical TOML-grep blocks in one file would
be a DRY finding. The checks operate on the normalized records, not the raw TOML. (The ts wrapper's
filter reads the GENERATED `.json`, never the TOML — so `parse_suppressions` is the SOLE TOML-parse
locus in the codebase.) For #47 this is an inline function in the check script (the only TOML
reader); extraction to `_audit_suppressions_lib.sh` is deferred per YAGNI unless a 2nd consumer
appears. (@code-reviewer confirmed: manifest parse stays INLINE single-caller — no shared helper,
that would be a one-caller abstraction violating YAGNI.)

**Derived-file readers — the ONE genuine 2-caller pair (@code-reviewer Gate-2 caveat):**
- `.cargo/audit.toml`: read by NOBODY in our code except the check script's sync-check — cargo-audit
  reads it NATIVELY (the rust wrapper does not parse it; that's load-bearing for security C2). So no
  duplication; sync-check's `.cargo/audit.toml` ignore-list parse is single-caller.
- `.pnpm-audit-ignore.json`: read by BOTH (i) the ts wrapper's post-hoc filter AND (ii) the check
  script's sync-check. **That is a real 2-caller pair** → extract ONE tiny helper
  `read_pnpm_ignore_ids` (reads the JSON `ignore[]` array → id list) consumed by both, rather than
  two JSON-parse blocks. (Both readers parse the SAME generated file for the SAME id list.) This
  keeps the derived-read DRY while the manifest parse stays inline. I'll implement it as the shared
  reader for that file.

### C. Derived file formats (confirm with @security)

- `.cargo/audit.toml` — cargo-audit native. Shape:
  ```toml
  [advisories]
  ignore = ["RUSTSEC-2023-0071"]
  ```
  Plus the verbatim rationale comment block above the `ignore` line (cargo-audit tolerates
  comments). cargo-audit reads `.cargo/audit.toml` by default, so the rust wrapper needs no flag
  change — it picks up the ignore list natively. This SATISFIES the "no CLI flag pass-through"
  security finding: suppression is sourced from a tracked file, never an ad-hoc `--ignore` flag.
- `.pnpm-audit-ignore.json` — custom post-hoc filter (pnpm has no native ignore file). Shape:
  ```json
  { "ignore": [] }
  ```
  (empty at seed — RUSTSEC-2023-0071 is rust-only). The ts wrapper runs `pnpm audit --json`, then
  filters advisories whose GHSA/advisory id is in this list before deciding pass/fail. Same
  tracked-file principle — no CLI flag.

### D. Conditional audit — `scripts/lang/{rust,ts}/audit.sh`

Add a dep-change gate at the top of each wrapper, BEFORE invoking the scanner:

**rust dep-manifest gate set (security-verified):** `Cargo.toml` (workspace root + any crate) +
`Cargo.lock`. **Gate implementation — SETTLED with @dry-reviewer, option (a):**
`diff_touches_root_files "Cargo.toml" "Cargo.lock"` **OR** `diff_touches_path "crates/"`. Rationale:
`_changed_helpers.sh` exposes only `diff_touches_path` (literal prefix) and `diff_touches_root_files`
(exact root) — there is NO changed-set glob predicate, and `crates/**/Cargo.toml` granularity would
need new parsing. Rather than reinvent diff-parsing in the wrapper (which @dry-reviewer would flag),
we gate on `diff_touches_path "crates/"` and ACCEPT the over-trigger: editing ANY crate source
re-runs the audit. That over-trigger is **fail-SAFE** — it's exactly the §D.1 "any doubt runs the
full scan" principle, reuses the existing helpers verbatim, and adds zero new parsing. (If
crate-manifest-only granularity is ever needed, the DRY-clean path is ONE new
`diff_touches_glob "crates/*/Cargo.toml"` predicate in `_changed_helpers.sh` + a test in
`_test_changed_predicates.sh` — kept in the single point of truth. Not needed for #47.)
**Committed: NO inline awk/grep/`git diff` parsing of the changed-files cache inside `audit.sh`** —
all changed-set detection goes through `_changed_helpers.sh` predicates.

**ts dep-manifest gate set (security-verified, decision 4 — COMPLETE set):**
- root `package.json`
- `pnpm-lock.yaml`
- `pnpm-workspace.yaml` ← **security added this**: editing it changes which packages resolve into
  the dep graph, so a change can alter the audited tree WITHOUT touching a `package.json`. Including
  it closes that gap.
- `packages/**/package.json` (glob — currently `packages/proto-gen` + `packages/test-utils`, but
  glob so new packages are auto-covered).
Security confirmed nothing else carries JS dependency edges today. **Gate implementation (same
helper-only, over-trigger-accepted shape as rust, per @dry-reviewer):**
`diff_touches_root_files "package.json" "pnpm-lock.yaml" "pnpm-workspace.yaml"` **OR**
`diff_touches_path "packages/"`. The `packages/`-prefix over-triggers (any package source edit
re-runs the ts audit) — fail-SAFE, helper-verbatim, zero new parsing, consistent with the rust gate.

**STATUS enum for the no-dep-changes skip — DELIBERATE DECISION (Gate 1, with @observability):**
The task literally says `STATUS=N/A REASON=no-dep-changes`, but N/A ranks **3** in
`aggregate_worst_status` (above OK=2), so an N/A audit would *dominate* a layer that otherwise
passed — turning "nothing to scan" into the layer's headline status. That contradicts the task's
*intent* (a cheap, non-masking skip). The correct exit-0 non-dominating signal already exists:
**`SKIPPED-NO-DIFF`** (rank **1**, below OK), which `_dispatch.sh` *already* emits via the
`changed.sh` short-circuit. **Decision: emit `SKIPPED-NO-DIFF REASON=no-dep-changes`, not N/A.**
Intent governs over the literal string. **CONFIRMED at Gate 1 by @observability (STATUS-contract
owner) AND @code-reviewer** (both converged on SKIPPED-NO-DIFF; N/A is enum-wrong — it means
"verb not yet wired / deliberate gap", but audit IS wired, just nothing to scan). @observability is
flagging to @team-lead that the task brief's literal `N/A` is enum-wrong so the brief isn't treated
as authoritative on this point. Documented here + in an in-code comment.

**REASON token `no-dep-changes` is the per-wrapper discriminator (kept — @observability):** the
ENUM (`SKIPPED-NO-DIFF`) is shared vocabulary; the REASON distinguishes audit's skip from a generic
lang skip. For rust/ts lint/test, `SKIPPED-NO-DIFF` means "this lang's source footprint untouched";
for AUDIT it means the narrower, security-relevant "no DEP-MANIFEST changed" — a strict subset. So
`STATUS=SKIPPED-NO-DIFF REASON=no-dep-changes` greppably answers "did the audit actually run, and if
not why" distinctly from a generic source-untouched skip. Keep the token.

**Truthfulness ↔ over-trigger interaction (reconciling @observability's honesty invariant with the
@dry-reviewer over-trigger gate) — IMPORTANT, no conflict:** @observability requires
`REASON=no-dep-changes` be honest — it must fire on a dep-manifest predicate, NOT the lang
`changed.sh` footprint (else a pure-source edit would mask a true no-dep-delta). Our gate uses
`diff_touches_path "crates/"` (DRY watch-point-1), which **over-triggers toward RUNNING** on any
crate-source edit — it never produces a FALSE skip. So the honesty invariant HOLDS in the direction
that matters: when audit emits `SKIPPED-NO-DIFF no-dep-changes`, the changed set is provably clean of
both `crates/` AND root `Cargo.toml`/`Cargo.lock` → genuinely no dep manifest changed → REASON is
truthful. The over-trigger only causes audit to occasionally RUN when a strict dep-manifest-only gate
would have skipped (a pure `crates/foo/src/lib.rs` edit runs the audit). That is fail-SAFE and never
emits a dishonest `no-dep-changes`. Net: a clean skip ALWAYS means no-dep-changes (honest); the
over-trigger only adds extra real runs, never false skips. If strict dep-manifest-only granularity is
ever wanted (so a pure-source edit also skips audit), that's the `diff_touches_glob
"crates/*/Cargo.toml"` predicate noted in §D — out of scope for #47. **@observability CONFIRMED this
reconciliation (zero reservations), declined the strict glob as scope-creep, and requires ONE in-code
comment** (no code change beyond the comment) on the wrapper's skip path stating the asymmetry, so a
3am operator knows OK-on-a-source-only-PR is expected, not a bug. Comment (commit verbatim/near-verbatim):
*"skip predicate is conservative: a clean SKIPPED-NO-DIFF guarantees no dep manifest moved; a
`crates/`-source-only edit RUNS audit (fail-safe over-trigger), it does not skip."* (ts wrapper gets
the analogous `packages/` comment.)

**Where the gate lives — SETTLED: (D-b), gate INSIDE `audit.sh`, KEEP `DEVLOOP_DISPATCH_ALWAYS_RUN=1`
(confirmed by @code-reviewer at Gate 1).** The audit dispatch **keeps `DEVLOOP_DISPATCH_ALWAYS_RUN=1`**
so `_dispatch.sh` invokes each `audit.sh` UNCONDITIONALLY; the wrapper then runs its OWN fail-closed
tri-state gate (§D.1) internally, calling shared `_changed_helpers.sh diff_touches_root_files` (NO
bespoke diff logic) and emitting `SKIPPED-NO-DIFF` via shared `_common.sh emit_status`.

**Why NOT the dispatcher short-circuit (rejected D-a):** the dispatcher's native short-circuit treats
a non-zero `changed.sh` as "untouched → skip" — that is **fail-OPEN for a security scan** (any
`changed.sh` error silently skips the audit). Dropping `ALWAYS_RUN=1` would hand gating back to that
fail-open path. @code-reviewer's #3 confirms: keep the wrapper invoked-always, gate internally,
fail-closed. The wrapper's tri-state (0=run, 1=skip, ≥2=run-on-doubt) is the safer shape and the
reason D-a is rejected despite its marginally-tighter DRY. The audit gate set is also the narrower
**dep-manifest subset** (Cargo.toml/Cargo.lock + pnpm set), distinct from `changed.sh`'s all-source
footprint — another reason it lives in the wrapper, not the dispatcher. DRY is still satisfied:
100% helper-delegated (`diff_touches_root_files` + `emit_status`), zero reinvented logic. (@dry-reviewer
confirming the reuse angle; @code-reviewer already confirmed the keep-ALWAYS_RUN shape.)

When the gate proves a clean no-dep-change → `emit_status SKIPPED-NO-DIFF no-dep-changes`, exit 0.
When deps changed OR the gate is indeterminate (§D.1) → run scanner; suppressed advisories (from the
tracked derived file) filter out of the failure list before pass/fail is decided.

**Applied-suppression visibility (observability requirement, REQUIRED):** when the scan runs and
filters one or more suppressed advisories out of the failure list, the wrapper emits a greppable
line naming the IDs actually applied THIS run — `SUPPRESSED=RUSTSEC-2023-0071[,<id>...]` (or one
line per ID). Rationale (@observability): a bare count tells a 3am operator "something was hidden"
but not *what*; the IDs make it actionable without re-deriving from the manifest, and close the
"suppressed advisory silently vanishes" gap. This line is SEPARATE from the `STATUS=` contract line
(does not touch the enum). If the scan runs and filters nothing, no `SUPPRESSED=` line is emitted
(absence = nothing suppressed). The always-run check MAY additionally emit a total active-suppression
count as a nice-to-have, but the per-run applied-IDs line is the load-bearing one.

**Early-warning (EXPIRING_SOON) — DEFERRED to TODO (CONFIRMED by @security policy call).** An
N-days-before-expiry warning is OUT of @observability's must-have set, OUT of #47 scope (#48 owns
remediation), and @security has explicitly deferred it: it's **non-posture** (the hard `expires`
FAIL is the load-bearing control, already in #47) — categorically different from the §F drift-catcher
(a hard co-sign condition). Its value also **depends on §F**: an EXPIRING_SOON line emitted only on
ad-hoc pipeline runs misfires (the suppression owner rarely sees it), so its natural home is a
fast-follow riding the §F scheduled job / a digest. If ever built, it MUST be a separate greppable
`EXPIRING_SOON=<id>:<days_left>` line with `STATUS` staying `OK` — **there is no `STATUS=WARN`** (§6
has no WARN member). Filed as a TODO with the `Depends-on: §F` note (see §K).

**Net-posture preservation**: the always-run guarantee MOVES from "scan every advisory every run"
to the cheap always-run `audit-suppressions-check.sh` date/hygiene gate. A drifted advisory DB that
publishes a NEW advisory against an UNCHANGED dependency would no longer be caught on an unrelated
diff — this is the real reclassification cost. Mitigations keeping posture from weakening:
  (i) the always-run date-check forces re-justification of every suppression on a fixed cadence;
  (ii) CI `push` to main always has a dep-manifest diff on dep-bump PRs, and the scheduled full
       scan (see §F) catches DB drift against unchanged deps;
  (iii) ADR-0033 §12 14-day MTTR tripwire is unchanged.
**This is an ADR-0033 §3 reclassification — needs explicit @operations + @security blessing at
Gate 1**, plus an ADR-0033 §3 amendment note (proposed text in §G).

### D.1 FAIL-CLOSED behavior (Lead must-have #3)

The conditional gate and the manifest parser are both safety-critical. Defaults bias toward
running the scan and toward "the suppression is unreadable", never toward silently skipping or
silently un-suppressing:

1. **Ambiguous / unresolvable changed-file detection → RUN the audit (don't skip).** The dep-change
   gate's "skip" path is taken ONLY on an affirmative, successful "no dep manifest changed" signal.
   If the changed-files cache is missing/unreadable, `_get_base_ref.sh` fails, the git base ref is
   unresolvable, or `diff_touches_*` returns a non-0/1 error code, the wrapper treats the gate as
   INDETERMINATE and FALLS THROUGH to running the scanner. Concretely: the gate helper returns an
   explicit tri-state — `0`=deps-changed-run, `1`=no-dep-change-skip, `≥2`=indeterminate; the
   wrapper runs the scan on `0` OR `≥2`, only emits `SKIPPED-NO-DIFF no-dep-changes` (NOT N/A — see
   §D STATUS-enum decision) on a clean `1`. So the
   only way to skip is a proven-clean no-dep-diff; any doubt runs the full scan. (This is stricter
   than the existing `changed.sh` short-circuit, which treats non-zero as "untouched" — the audit
   wrapper deliberately does NOT inherit that, because skipping a security scan on ambiguity is the
   exact failure we must avoid.)

2. **Malformed file → SPLIT fail-mode (security decision, FINAL).** @security adjudicated the one
   genuine decision I left open: the filter and the check are DIFFERENT code paths with DIFFERENT
   correct directions. Encode both exactly:
   - **`audit-suppressions-check.sh` (the always-run check) → HARD-FAIL loudly.** It distinguishes
     "manifest absent/empty" (legitimate: zero suppressions, OK) from "manifest present but
     unparseable" (a `[[suppression]]` table missing a required field, malformed line, bad `expires`
     shape, duplicate id, etc.) → `STATUS=FAIL REASON=suppression-malformed` (the single @security
     token; the message names whether it's the manifest or a derived file —
     `.cargo/audit.toml`/`.pnpm-audit-ignore.json` — and the offending line). The parser NEVER
     degrades a malformed entry into "0 suppressions parsed" — a failed-to-parse entry is a FAIL,
     not a silently-dropped row. This is the backstop that forces
     the drift to get fixed and goes red even on a no-dep PR.
   - **The ts wrapper's POST-HOC FILTER → fail-SAFE (apply ZERO suppressions), do NOT abort the
     scan.** If `.pnpm-audit-ignore.json` is malformed, the filter treats the ignore-list as EMPTY
     (suppress nothing) so EVERY advisory surfaces and the audit can still go RED on a real vuln —
     fail-closed *for the security gate*. It must NOT hard-abort the scan, because a malformed
     ignore-file blocking the audit from running would be a different denial-of-coverage failure.
     On the malformed path the filter emits a **one-line `WARN` to stderr** (verbatim, @security):
     `WARN: .pnpm-audit-ignore.json unparseable — applying ZERO suppressions (all advisories will
     surface); audit-suppressions-check will FAIL separately. Fix the file.` then proceeds — it does
     NOT touch the `STATUS=` contract (no `STATUS=WARN`; §6 has no WARN member) and does NOT abort.
     The loud hard-fail for that same malformed file comes from the always-run check separately.
     In-code comment (verbatim, per @security + @code-reviewer): *"malformed ignore-file ⇒ apply zero
     suppressions (fail-safe); audit-suppressions-check hard-fails separately."*
   - **Net effect** (both directions safe): a malformed derived file can never silently silence an
     advisory (filter applies zero suppressions) AND can never go unnoticed (the always-run check is
     RED). This REVISES my earlier "filter fails loud" lean — security's split is the correct shape.
   - **PATH-1 acceptance test (security override, Gate-2 verified):** write a malformed
     `.pnpm-audit-ignore.json`, inject a known advisory into the pnpm tree, run the ts wrapper → the
     audit MUST FAIL on the advisory (proving the scan RAN with zero suppressions), NOT error-out
     before scanning. An implementation that errors before scanning is the denial-of-coverage bug
     and fails Gate 2 (both @security + @code-reviewer flag a filter-that-aborts-the-scan as a
     finding). Added to the §I test matrix.

3. **Sync-check drift is FAIL, not auto-heal.** Without `--fix`, a manifest that doesn't match the
   derived files FAILs (it does not silently regenerate), so a hand-edited derived file or a
   forgotten `--fix` can never silently win over the manifest.

**@semantic-guard Gate-2 verification bar (recorded):** every FAIL above must be a genuine
**non-zero exit**, not merely a printed `STATUS=FAIL`/warning line — `suppression-drift`,
`suppression-past-due`, `suppression-quality`, `suppression-malformed`,
`suppression-override-without-test-sentinel`, and `test-sentinel-set-in-ci` all return non-zero so
`run_and_emit`/the guard/`layer3.sh`/`layer-all.sh` actually go red.
And **no path may downgrade a real audit failure to OK/SKIPPED/N/A**: the SKIPPED-NO-DIFF skip is
reachable ONLY on a proven-clean no-dep-diff (§D.1 tri-state), never on scanner error, parse error,
or ambiguity. Semantic-guard verifies both in the Gate-2 diff. (This is exactly the fail-closed
contract; calling it out so the implementation keeps the exit-code and the printed STATUS in
lockstep — a `STATUS=FAIL` line with a zero exit would be the precise bug they're watching for.)

### E. Wiring — CI + devloop pipeline

**Decision: wire `audit-suppressions-check.sh` into Layer 3 (Guards), not Layer 6 (Audit) —
structurally separate from the now-conditional scan.** @security made this a non-negotiable
(their msg 2026-06-06): the always-run date-check must NEVER inherit the conditional scan's
skip-if-no-dep gating, or the backstop silently stops firing on no-dep PRs and the §3-amendment
blessing collapses. The invariant @security requires: **"date-check fires on a no-dep-change PR in
CI AND devloop"**, on a code path independent of the scan.

**Placement mechanism — DECISION: a thin guard under `scripts/guards/simple/`, not a direct
`layer3.sh` line.** @security framed Layer 3 as "`simple/**/*.sh` via run-guards", and that IS the
idiomatic always-run home: `run-guards.sh` auto-discovers `simple/**/*.sh` (recursive,
fixtures-pruned), runs each in BOTH CI and devloop inside the 30s per-guard timeout, and is itself
the always-run Layer-3 body. So:
  - **New guard `scripts/guards/simple/audit-suppressions.sh`** — a thin wrapper that invokes
    `scripts/audit-suppressions-check.sh` (the real logic stays in `scripts/`, the guard is the
    Layer-3 entry point, mirroring how `simple/*.sh` dt-guard wrappers delegate to `crates/dt-guard`).
    Caveat handled: `run-guards.sh` calls every guard as `guard "$SEARCH_PATH"` (repo root); the
    wrapper must tolerate/ignore that positional arg (it does not pass it through to the check, which
    operates on fixed repo-root paths). The guard runs the check WITHOUT `--fix` (read-only verify).
  - This keeps the date-check provably independent of the conditional Layer-6 scan: different layer,
    different runner (`run-guards.sh` vs the audit dispatcher), no shared gating. Satisfies C1.
  - **Rejected alternative**: a direct `run_and_emit` line in `layer3.sh` (like the
    `_test_changed_predicates.sh` meta-test). Workable, but less idiomatic than the guards dir and
    doesn't match @security's "simple/**/*.sh" framing. The guards-dir home is the structurally
    correct one. (The `*.test.sh` meta-test for this script — §I — is a separate concern: that's a
    test, not a guard; it gets its own wiring, see §I.)

**CI**: `scripts/layer-all.sh` runs all layers including Layer 3 → run-guards → the new guard, so CI
coverage is automatic and ALWAYS-RUN (Layer 3 has no skip). The task's "always-run in CI" is
satisfied by the guard executing every CI run. **q4 RESOLVED — @operations: NO explicit ci.yml
step.** Adding a second hand-wired ci.yml step that runs `audit-suppressions-check.sh` directly would
reintroduce exactly the router-drift ADR-0033 §10 (Single Source of Truth) forbids — two invocation
paths that can silently diverge. Layer 3 via `layer-all.sh` is the sole CI path; no duplicate step.
**Operations co-sign condition C1 (replaces the "assumed" coverage):** because there's no explicit
CI step, I MUST PROVE the check fires through `layer-all.sh → layer3.sh → run-guards → guard`. The
self-test (§I) feeds a past-due fixture manifest and asserts (a) the guard runs in the Layer-3 path
and (b) an expired entry makes `layer3.sh` exit non-zero (red-on-expiry). "It runs in CI" is
verified, not assumed. See §I + §J for the exact self-test + layer3-non-zero assertion.

**Time-bomb-by-design (per @security):** the date-check turns CI red the day an entry hits
`expires` — intended behavior, not a flake. Commitments: (a) the FAIL message is actionable
(days-past-due + offending IDs + "either re-suppress with a new date or fix the advisory"); (b) a
one-line runbook note documents the red-on-expiry behavior so on-call doesn't treat it as a CI
break. Runbook location is operations-owned — I'll add the note where @operations directs (likely
`docs/runbooks/devloop-validation.md` §6.3 Layer 3, alongside the guard triage). @security +
@operations co-confirm the runbook angle.

### F. Scheduled full scan (posture backstop) — IN-SCOPE (operations co-sign condition C2)

**RESOLVED: @operations rules §F IN-SCOPE for #47 (co-sign condition C2), and the reason is
load-bearing, not nice-to-have.** I verified @operations' finding: there is **NO Dependabot config**
in this repo (`.github/dependabot.yml`/`.yaml` both absent), and `fuzz-nightly.yml` is the only
scheduled workflow. So my §D net-posture argument's mitigation (ii) leaned on a scheduled scan AND
an implicit Dependabot — **neither exists today**. Without §F, the only thing catching a
newly-published advisory against an unchanged lockfile is the next human dep-bump PR (possibly weeks
out) — a real posture weakening, and exactly the minimatch failure class ADR-0033 was written to
close. **With no Dependabot, the scheduled scan is load-bearing.**

**Deliverable (this task) — SETTLED: WEEKLY scheduled scan (@operations decision).** A minimal
scheduled workflow `.github/workflows/audit-scheduled.yml` (`schedule: cron` **WEEKLY**, modeled on
`fuzz-nightly.yml`) that runs the **UNCONDITIONAL** `cargo audit` + `pnpm audit` — the audit wrappers
with the dep-gate forced ON. **Cadence rationale (@operations):** weekly, not nightly — a
Marvin-class transitive advisory does not need <24h detection, weekly keeps CI-minutes down, and
weekly ≪ the ADR-0033 §12 14-day MTTR tripwire, so it aligns with the existing MTTR window.
Mechanism: a `DEVLOOP_AUDIT_FORCE_RUN=1` env override the conditional gate in `audit.sh` checks
FIRST — when set, the gate is bypassed and the scan always runs (suppressions STILL filter via the
tracked derived files; this is a GATE bypass, NOT a suppression bypass). The scheduled job sets the
override + runs the audit path. So the scheduled scan surfaces NEW/unsuppressed advisories against
unchanged deps within ≤1 week — the drift-catcher that closes the diff-less vector §3 cared about.

**`DEVLOOP_AUDIT_FORCE_RUN` is force-RUN-ONLY (security invariant, locked).** Hard constraints on the
env var, verified at Gate 3:
  - It can ONLY turn a would-be `SKIPPED-NO-DIFF no-dep-changes` into a full RUN. There is **no
    force-SKIP** semantics — no `DEVLOOP_AUDIT_FORCE_RUN=0` path and no sibling var that skips the
    scan. The gate logic: `if force-run set → run; else → tri-state gate (§D.1)`. The only effect of
    the var is "run even when the gate would skip"; absence/any-non-set value leaves the normal
    tri-state untouched. There is NO env input that can make the scan SKIP.
  - It does NOT force-suppress and does NOT pass through to `cargo audit`/`pnpm audit` as a flag
    (preserves the Wave-1 no-CLI-pass-through finding — an env-based gate-silencer would be the same
    bypass class). Suppression on the scheduled path flows ONLY through the manifest → generated
    derived files — the **SAME tracked suppression set** as every other run, never a different or
    empty set. The scheduled scan is the production suppression set + a forced run, nothing else.
  - Net: the var is a pure run-amplifier (toward more scanning), structurally incapable of skipping
    or silencing. That's the only safe direction for an env lever on a security gate.

**DECISION SETTLED — LEAD RULING (user): land BOTH controls in #47.** Earlier ops+security
recommended scan-only; the Lead/user ruled **BOTH** — (1) the weekly scheduled full-scan
(`audit-scheduled.yml`, the drift-catcher above) AND (2) `.github/dependabot.yml` (cargo + npm/pnpm,
weekly) for bump/remediation hygiene. "Both" is a SUPERSET of scan-only, so it satisfies the
ops+security no-weakening condition (scan-only was their floor, not a ceiling) — but the dependabot
second-suppression-surface reconciliation (§F.2) is a NEW item @security must bless. @security called
the scheduled scan *"the single most important compensating control"*; dependabot adds proactive
bump PRs on top. Expanded-scope re-confirmation requested from @operations + @security before Lead
issues "Plan approved."

### F.1 Scheduled-scan failure surfacing — deduplicated GitHub Issue (Lead design point A)

The user asked "what's the output?" The only existing cron workflow (`fuzz-nightly.yml`) relies SOLELY
on a red run + `::error::` annotation + GitHub's default cron-failure email — which goes to ONLY the
schedule's last-editor. **Too weak for a security drift-catcher** (the one person who last touched the
cron file is not necessarily who needs to act on a new advisory). So `audit-scheduled.yml` must, ON
FAILURE, **open-or-update a single deduplicated GitHub Issue** (label `audit-drift`, ONE rolling
issue — not one-per-run) IN ADDITION TO the red run + `::error::` annotation. Mechanism:
`actions/github-script` (already a first-party action — NO new marketplace dependency) does the
open-or-update: search for an open issue with the `audit-drift` label; if found, append a comment with
the new run's advisory output + timestamp; else create it. The issue body carries the scan output
(the unsuppressed advisories that drifted in) + a pointer to `docs/contributor/audit-suppressions.md`
(suppress-vs-fix). On a subsequent GREEN scheduled run, optionally close/auto-resolve the rolling
issue (or comment "resolved as of <run>"). The suppression manifest STILL applies on this path
(`DEVLOOP_AUDIT_FORCE_RUN` bypasses the dep-change gate, NOT the suppressions) — the issue only fires
on genuinely unsuppressed drift. `scripts/audit-scheduled.yml` (+ the github-script step) is Mine/infra.

### F.2 Dependabot ↔ suppression-manifest reconciliation (Lead design point B — @security BLESSES)

Adding `.github/dependabot.yml` introduces a **SECOND suppression surface**: Dependabot has its own
alert-dismissal flow and a `ignore:` block, which COMPETE with `audit-suppressions.toml` as the
single source of truth. Left unmanaged, an advisory could be "suppressed" via a Dependabot alert
dismissal that lives nowhere in-tree — fragmenting the SSOT and defeating the whole point of the
manifest. **Reconciliation (must be @security-blessed):**
  - **`audit-suppressions.toml` is the ONLY sanctioned place to suppress an advisory.** Dependabot is
    for **bump PRs only** — its alert-dismissal / `ignore:` MUST NOT be used as a suppression channel.
    Documented prominently in `docs/contributor/audit-suppressions.md` (§K) and as a comment header in
    `dependabot.yml` itself pointing at the manifest.
  - **`dependabot.yml` carries NO `ignore:` entries** (an empty/absent ignore block) — so it cannot
    become a shadow suppression list. If a transitive advisory needs suppressing, it goes in the
    manifest, full stop. (The manifest → `.cargo/audit.toml`/`.pnpm-audit-ignore.json` derived files
    govern what the AUDIT silences; Dependabot only opens bump PRs and surfaces alerts — it does not
    gate CI, so its alerts and our manifest don't collide on the gate, they collide only as
    *suppression channels*, which the doc+no-`ignore:` discipline closes.)
  - **Manual enablement caveat:** Dependabot *security-updates* (the auto-PR-on-advisory behavior, vs
    plain version-updates) require a repo **Settings toggle that cannot be committed**. Call this out
    in the contributor doc as a manual enablement step (committing `dependabot.yml` enables
    version-updates; security-updates need the Settings toggle flipped by a maintainer).
  - **Config:** `dependabot.yml` declares two ecosystems — `cargo` (root) + `npm` (pnpm workspace;
    Dependabot's `npm` ecosystem handles pnpm lockfiles), both `schedule: weekly`. Comment header
    points at the manifest as the sole suppression channel.
  `.github/dependabot.yml` is Mine/infra; the second-surface POLICY (no-`ignore:`, manifest-is-SSOT)
  is @security's call — hence the Gate-1 re-bless ask.

### G. ADR-0033 §3 amendment note — MANDATORY (Gate 1 must-have #3)

**This is non-optional.** The conditional scan directly REVERSES the §3 Alternatives-Considered
entry *"Per-toolchain Always-Run audit only on lockfile touch — Rejected"* (ADR-0033 line ~460),
whose rejection rationale was: *"the minimatch incident proved transitive vulns surface independent
of lockfile diffs in the current branch (they land via Dependabot or sibling devloops merging to
main)."* The amendment must answer that exact rationale head-on with a named compensating control,
co-signed by @operations + @security. @team-lead verifies presence at Gate 2.

Amendment lands as a new dated subsection in ADR-0033 §3 (and the rejected-alternative line gets a
back-reference "superseded by the task-#47 amendment, see §3"). Proposed text:

> **Amendment (task #47, 2026-06-06) — audit scan reclassified to dep-change-gated.**
> `cargo audit` / `pnpm audit` are reclassified from unconditional always-run to
> **dep-change-gated**: when no dependency manifest (`Cargo.toml`/`Cargo.lock`,
> `package.json`/`pnpm-lock.yaml`) is in the changed-file set, the wrapper emits
> `STATUS=SKIPPED-NO-DIFF REASON=no-dep-changes` (exit 0, non-dominating — *not* N/A; see §D
> rationale). This reverses the previously-rejected "audit only on lockfile touch" alternative.
>
> **Why the original rejection no longer blocks this — a compensating control that did NOT exist at
> ADR-authoring time.** The §3 "audit only on lockfile touch" alternative was rejected *because, at
> the time ADR-0033 was written, there was no other mechanism catching transitive vulns that surface
> without a current-branch lockfile diff* — so gating the scan on lockfile touch would have left that
> vector completely uncovered (the minimatch incident). Task #47 ADDS the mechanisms that were
> missing then, which is precisely what makes the reversal legitimate now rather than a regression:
>   1. **Any dep-bump PR carries a lockfile diff** → those PRs run the full scan (the gate fires
>      true). A dep bump *is* a manifest change, so the deliberate-dependency-change vector is never
>      a gap. (#47 ALSO adds `.github/dependabot.yml` (cargo + npm/pnpm, weekly) per the Lead "Both"
>      ruling — so bump PRs are now partly automated too; the argument holds for any dep-changing PR
>      regardless of who/what opens it. Dependabot is for bump PRs only — suppression stays
>      manifest-only, §F.2. The genuinely diff-less vector is handled by #2.)
>   2. **Sibling-merge / advisory-DB-drift against an UNCHANGED dependency** (the genuinely
>      diff-less vector — the exact gap that justified the original rejection) is now covered by the
>      **WEEKLY scheduled full `cargo audit` + `pnpm audit`** (§F) that catches new advisories within
>      ≤1 week regardless of any branch diff, PLUS the **always-run `audit-suppressions-check.sh`
>      date-check** (Layer 3) that hard-fails any expired suppression every run, on every diff,
>      forcing periodic re-justification. Neither existed when §3 was authored.
>   3. ADR-0033 **§12 14-day MTTR tripwire** is unchanged and still applies (weekly scan ≪ 14d).
>
> **Net posture:** the always-run *guarantee* moves from "scan every advisory every run" to "cheap
> always-run date/hygiene discipline + weekly scheduled full scan". The per-PR cost drops (and the
> no-dep-change always-run wall-clock is budget-POSITIVE — Layer 6 no longer scans on no-dep PRs);
> the only thing lost is "catch a brand-new advisory against an unchanged dep *on an unrelated PR
> within minutes*", which the weekly scheduled scan recovers within ≤1 week — an explicitly-accepted
> trade (operations + security joint co-sign). Suppression is sourced exclusively from the tracked `audit-suppressions.toml`
> manifest and its generated derived files — never ad-hoc CLI flags (preserves the Wave-1
> no-pass-through security finding).

**Dependency RESOLVED:** the amendment's "covered by the scheduled full scan" claim is load-bearing,
and @operations made §F a co-sign condition (C2) → **§F lands in this task**, so the claim is TRUE
as written (not overstated). Per @operations' required edit: the amendment must NOT say "covered"
unless §F (or a committed `.github/dependabot.yml`) actually lands here — it does (C2), so the text
stands. If Lead were to override and force §F fully out with no dependabot fallback, this amendment
text MUST be rewritten to say the diff-less-vector gap is **OPEN and tracked**, not "covered", and
operations+security re-bless under that weaker posture. I will not land the reclassification with an
overstated amendment.

### G.1 Security's non-negotiable posture invariants (decision C) — the machinery MUST honor

@security blesses the §3 reclassification ONLY if the machinery satisfies all three. All three are
already in the design; restating as acceptance criteria @security verifies at Gate 2:

- **C1 — suppressions-check is genuinely ALWAYS-RUN** (date + sync + quality), firing even when zero
  deps changed, in BOTH CI and devloops, on a code path STRUCTURALLY INDEPENDENT of the conditional
  scan (it must not inherit the scan's skip-gating). This is the new discipline backstop replacing
  the always-run scan guarantee. The date-check firing on a past-due `expires` with NO dep change is
  exactly the §3 externality (the calendar advances independent of our diff). → satisfied by §E:
  a Layer-3 guard `scripts/guards/simple/audit-suppressions.sh` (auto-discovered by `run-guards.sh`,
  always-run, no skip, different runner than the audit dispatcher) + §B date-check. Independence is
  structural — different layer, different runner, zero shared gating.
- **C2 — suppression sources ONLY from the tracked manifest → generated derived files.** The
  existing no-CLI-pass-through header comments in `scripts/lang/{rust,ts}/audit.sh` are preserved
  VERBATIM (no `--ignore`/`--audit-level` pass-through) and EXTENDED to point at the manifest as the
  SOLE suppression channel. Ad-hoc CLI suppression stays impossible. → satisfied by §C
  (cargo reads `.cargo/audit.toml` natively, no flag) + §D (ts filter reads the tracked `.json`).
- **C3 — sync-check HARD-FAILS on derived-file drift** (hand-edited `.cargo/audit.toml` adding/
  removing an id vs the manifest). Drift = FAIL, not warn. Closes the "edit the derived file
  directly" bypass. → satisfied by §B sync-check + §D.1(3).

Security's framing (recorded): a dep-change-gated scan + always-run hygiene/date check is STRONGER
than today's always-run-scan-with-no-expiry-enforcement, because today NOTHING forces a suppression
to expire. The amendment ADDS expiry teeth we don't currently have. This is the core
net-posture-not-weakened argument @operations co-signs.

### G.2 Operations co-sign conditions (decision, 2026-06-06) — OC1–OC5

@operations **BLESSES** the §3 reclassification (Q3) with the §G amendment + these five conditions.
(Labeled OC# to avoid collision with security's C1–C3 above.) None require redesign — all are
additions to the already-shaped plan.

- **OC1 — PROVE the check fires through CI** (replaces "assumed coverage" now that q4 = no explicit
  ci.yml step). The self-test (§I/§J) must assert (a) the guard runs in the Layer-3 path via
  `layer-all.sh → layer3.sh → run-guards`, and (b) a past-due fixture manifest makes `layer3.sh`
  exit non-zero (red-on-expiry). "It runs in CI" = verified, not assumed. → §J.
- **OC2 — §F scheduled full-scan lands in THIS task** (Dependabot absent → scheduled scan is
  load-bearing). Weekly OK; runs UNCONDITIONAL cargo+pnpm audit. Fallback (only if Lead hard-rules
  §F out): commit `.github/dependabot.yml` (cargo+npm) here instead. One MUST land. → §F.
- **OC3 — runbook entry** in `docs/runbooks/devloop-validation.md` §6.3 (Layer 3) for the new
  failure shape "audit-suppressions-check FAIL: past-due suppression": documents that the red is
  INTENTIONAL (CI red the day an entry expires), the exact message shape, and the two remediations
  (re-suppress w/ new `expires` + re-run `--fix`, OR fix the advisory). → added to deliverables.
- **OC4 — actionable past-due message**: days-past + offending IDs + literal "either re-suppress
  with new date or fix" + **a pointer to `docs/contributor/audit-suppressions.md`** for the renewal
  workflow. (First three already in §B; ADD the doc pointer.) → §B updated.
- **OC5 — `docs/contributor/audit-suppressions.md` renewal workflow must be REAL + end-to-end
  followable**: edit manifest `expires` → run `scripts/audit-suppressions-check.sh --fix` → commit
  BOTH manifest + regenerated derived files. Document the 90-day default explicitly. @operations
  verifies followability at review. → §K (contributor doc) updated.

### G.3 Scope clarification (operations) — §F is no longer a "scope question"

@operations escalated §F from "open scope question" to a **co-sign condition** (OC2). So the Lead's
remaining role on §F is NOT in/out — it's confirming **scheduled-scan (recommended) vs
`.github/dependabot.yml` fallback**. I recommend the scheduled scan. Either way, one lands in #47.

### H. Self-classification (Cross-Boundary Edits)

Per the upgraded §Cross-Boundary table: the policy-content paths (`audit-suppressions.toml`,
`.cargo/audit.toml`, `.pnpm-audit-ignore.json`) AND the two `audit.sh` wrappers (filtering +
gate change) are **Not-mine / Minor-judgment (security)** — hunk-ACK `Approved-Cross-Boundary:
security <reason>` trailers, co-signed by @security at Gate 2. Machinery
(`audit-suppressions-check.sh`, ci/layer wiring, contributor doc, TODO mirror) is **Mine**.
No GSA paths.

### I. Testability + DRY commitments (Gate 1 — @test + @dry-reviewer)

**@test determinism bar — THREE REQUIRED fold-ins (all accepted, stated explicitly):**

- **(T1) Injectable reference `now` — REQUIRED, load-bearing.** The check script reads "today" from
  an injectable seam: `NOW="${AUDIT_SUPPRESSIONS_NOW:-$(date -u +%F)}"` (env override, defaults to
  the real clock in prod). Without this seam a clock-dependent date test sees only ONE calendar
  point → the "future→OK" case rots the day the 2026-09-01 seed goes past-due, and the
  `expires==today` boundary can't be tested deterministically. The self-test sets
  `AUDIT_SUPPRESSIONS_NOW=2026-06-06` and asserts fixed-input cases. **Semantic (stated + commented
  in-code):** `expires == today` is **VALID** (not yet past) — date-check FAILs only on strictly
  `expires < NOW`. Tests both `today` (OK) and `today-1` (FAIL).
- **(T2) Self-test WIRE-IN — stated explicitly.** @test independently traced the same finding: NO
  `find -name '*.test.sh'` auto-runner exists; the existing self-tests run only because a layer
  invokes them (`_test_changed_predicates.sh` is called explicitly from `scripts/layer3.sh`). So
  `scripts/audit-suppressions-check.test.sh` is ORPHANED unless I ALSO add
  `run_and_emit "audit-suppressions-test" "${__here}/../audit-suppressions-check.test.sh"` to
  `layer3.sh`, right beside the predicate meta-test. **This wire-in is committed** (§J also depends
  on it). Note: the `test-registration` guard only covers Rust `crates/*/tests/`, so it provides no
  coverage here — the explicit layer3 line is the only thing that runs this test.
- **(T3) Consume `_test_helpers.sh`, don't hand-roll.** Canonical shape is
  `scripts/lang/proto/changed.test.sh`: `source ../_test_helpers.sh` then assertions +
  `report_results`. `_test_helpers.sh` today has `assert_rc` (exit-code) + `run_with_cache` +
  `report_results`, but NO STATUS-line assertion. My tests assert STATUS + exit-code, so I'll **add
  thin `assert_status`/`assert_exit` helpers to `_test_helpers.sh`** (shared, not inline) and consume
  them — avoids the 4th-hand-rolled-scaffold DRY finding @test would raise. **Each new helper gets a
  one-line doc-comment in the same style as the existing `assert_rc` comment** (@test's ask) so the
  next consumer finds it. `run_with_cache`'s synthetic `DEVLOOP_TMP/changed-files.layer-locality`
  injection is exactly the hermetic seam the conditional-gate tri-state tests need.

**Test matrix (@test's list — the diff is checked against this at review):** New file
`scripts/audit-suppressions-check.test.sh`, table-driven via `_test_helpers.sh`:
- date-check: `today-1` → FAIL (1 day past, ID listed); `today` (==expires) → OK (boundary, valid);
  far-future → OK; **multiple past-due** → FAIL where the test ASSERTS each ID AND its OWN distinct
  days-past appear in the message (@test: e.g. two entries at `today-1` and `today-30` → assert both
  IDs present AND "1" and "30" days each — NOT merely that >1 entry failed; checking only the first
  ID under-tests, and @test will grep for both at review).
- empty/absent manifest → OK (vacuously, 0 suppressions).
- malformed manifest (missing `expires`, bad date format, missing reason/ticket, bad ecosystem) →
  quality/parse FAIL (a test row per malformation).
- **sync-check BOTH directions, BOTH derived files**: id in manifest missing from derived → FAIL;
  id in derived not in manifest → FAIL; for `.cargo/audit.toml` AND `.pnpm-audit-ignore.json`.
- **`--fix` idempotence**: run twice → second run byte-identical (no churn); a dirtied derived file
  is actually re-synced by `--fix`.
- conditional-gate tri-state (deps-changed→run, proven-clean→SKIPPED-NO-DIFF, indeterminate→run)
  via `run_with_cache`-style synthetic changed-files cache.
- **ts-filter malformed-file fail-SAFE (PATH-1, security override §D.1.2):** malformed
  `.pnpm-audit-ignore.json` + a known advisory in the pnpm tree → the ts wrapper RUNS the scan and
  FAILs on the advisory (zero suppressions applied) + emits the stderr WARN — it does NOT error
  before scanning. This is the denial-of-coverage guard; asserts the scan ran, not aborted.
- **sentinel-gating A+B (§J, security trust-boundary):** (a) override env set + `DEVLOOP_TEST=1` →
  check reads the override (fixture used) — proves the test path works; (b) override env set +
  `DEVLOOP_TEST` ABSENT → check FAILs `suppression-override-without-test-sentinel`; (b') override env
  set + `DEVLOOP_TEST=0` (WRONG value) → check ALSO FAILs `suppression-override-without-test-sentinel`
  (proves the EXACT-match `== "1"` closes the truthy bypass — this is the load-bearing B case);
  (c) NO override + NO sentinel → check uses hard-wired repo-root manifest + real date (production
  default inert to ambient env). Same set for the ts-filter derived-path override.
- **CI-sentinel-leak assertion C (§J, operations):** simulate CI with `GITHUB_ACTIONS=true` +
  `DEVLOOP_TEST` set → `layer-all.sh` (and/or `layer3.sh`) HARD-FAILs early with
  `STATUS=FAIL REASON=test-sentinel-set-in-ci`; and `GITHUB_ACTIONS=true` + `DEVLOOP_TEST` unset →
  no such failure (assertion is leak-specific, not a false trip on normal CI).

**FAIL-branch coverage is a SECURITY requirement (@security, verified at Gate 3):** the guard
running green every devloop does NOT exercise the FAIL paths — so the self-test MUST drive the
security-relevant FAIL branches with fixtures: a past-due fixture actually FAILs (non-zero), a
malformed manifest actually FAILs, sync-drift actually FAILs. @security's framing: "an always-run
check whose FAIL path is never tested is a check I can't trust to fire when it matters." Test owns
the depth bar; @security specifically cares that the FAIL branches (not just the green path) are
exercised. The OC1/§J past-due-fixture → `layer3.sh` non-zero assertion is the load-bearing one for
the date-check FAIL branch; the matrix above covers the sync/quality/parse FAIL branches.

- **WIRED IN**: an unrun test is untested. **FINDING (investigated):** there is currently NO
  automatic discovery/runner for the shell `*.test.sh` meta-tests. Only
  `scripts/lang/_test_changed_predicates.sh` is explicitly invoked (by `layer3.sh`). The other
  meta-tests (`_dispatch.test.sh`, `_common.test.sh`, `rust/ts/changed.test.sh`,
  `_get_base_ref.test.sh`, etc.) are run *manually* — they are not wired into any layer or CI. So
  "register the new test with the existing runner" has no existing runner to register with for the
  general case.
  **NOTE — distinct from the §E guard wiring:** §E wires the *production* check (a Layer-3 guard
  `simple/audit-suppressions.sh` that VERIFIES on every run). This §I item is the *meta-test* of the
  check script itself (does past-due correctly FAIL, etc.) — a different artifact. The guard running
  green every devloop does NOT exercise the FAIL paths; only the meta-test does. Both must run.
  **SETTLED (@test ACCEPTED):** add an explicit `run_and_emit "audit-suppressions-selftest"
  "${__here}/../audit-suppressions-check.test.sh"` invocation to `layer3.sh` (alongside the existing
  `_test_changed_predicates.sh` meta-test line), so the self-test runs every devloop AND in CI
  (layer-all → layer3). @test independently re-verified the orphaned-meta-tests finding and confirms
  this is the correct fix — NOT deferred. The broader `*.test.sh` glob-runner is an ACCEPTED
  spin-out (deferred), filed as a TODO per §K (@test condition 1); @test's verdict is
  RESOLVED-DEFERRED. The layer3 wire-in for #47 stays regardless of the glob-runner (@test
  condition 2).

**@dry-reviewer commitments (accepted):**
- **Single shared manifest-parse helper**: one function (e.g. `_parse_suppressions` in the check
  script, or a sourced `scripts/lang/_audit_suppressions_lib.sh` if reused by the ts filter) is the
  ONLY place that reads `audit-suppressions.toml`. The date/sync/quality checks and `--fix` all call
  it; no second TOML parser anywhere. The ts wrapper's filter reads the *derived* `.json` (already
  generated), so it never re-parses the TOML — single parse locus preserved.
- **Reuse `_changed_helpers.sh`** (`diff_touches_root_files`) for the dep-manifest gate — no bespoke
  diff logic. **Reuse `_common.sh` `emit_status`/`run_and_emit`** for all STATUS emission — no
  reinvented STATUS lines.

### J. CI-fires-the-check proof (operations OC1) — verified, not assumed

Because q4 = no explicit ci.yml step, "the check runs in CI" must be PROVEN. Two assertions, both in
the self-test (`scripts/audit-suppressions-check.test.sh`) so they run every devloop + CI:

- **J-a (path)**: assert the guard `scripts/guards/simple/audit-suppressions.sh` is discovered and
  invoked by `run-guards.sh` (and thus by `layer3.sh` → `layer-all.sh`). Concretely: the self-test
  runs `scripts/guards/run-guards.sh scripts/guards/simple/audit-suppressions.sh` (or asserts the
  guard appears in run-guards' discovered set) and confirms a non-zero exit propagates.
- **J-b (red-on-expiry through the layer)**: with a **past-due fixture manifest** injected (via the
  TEST-ONLY `DEVLOOP_SUPPRESSIONS_MANIFEST` path override + `AUDIT_SUPPRESSIONS_NOW` set to a date
  after `expires`), assert that **`layer3.sh` exits NON-ZERO**. This proves the red-on-expiry
  actually surfaces through the real Layer-3 path that CI runs — not just that the check script
  returns 1 in isolation.

**TEST-INJECTION envs — SENTINEL-GATED (security REQUIREMENT, tighter variant — replaces the
omit-from-caller approach).** @security identified a real gap in "the guard invokes the check with no
override": env vars are PROCESS-INHERITED, so the check reads `DEVLOOP_SUPPRESSIONS_MANIFEST` (and
`AUDIT_SUPPRESSIONS_NOW`, and the ts-filter derived-path override) from its OWN ambient environment
regardless of whether the guard "passes" it. A compromised/misconfigured CI step exporting
`DEVLOOP_SUPPRESSIONS_MANIFEST=/tmp/attacker.toml` before Layer 3 runs would point the always-run
check at an attacker-controlled manifest = the exact CI backdoor (silent suppression of live
advisories). "Guard omits it" only audits the caller; the security property required is that the
always-run path is **INERT TO THE AMBIENT ENVIRONMENT, provable by reading the check script alone.**

REQUIRED shape — A+B+C (security FINAL, operations co-confirmed; all three required):

**A. Sentinel-gated overrides (off-by-default).** All test-injection envs
(`DEVLOOP_SUPPRESSIONS_MANIFEST`, `AUDIT_SUPPRESSIONS_NOW`, the ts-filter derived-path override) are
read ONLY when the test sentinel is set in the SAME process. Absent it → hard-wired repo-root
`audit-suppressions.toml` + real `date -u` + repo-root generated `.pnpm-audit-ignore.json`. This is a
property of the CHECK ITSELF (override read gated on the sentinel), not of the caller. The production
guard `scripts/guards/simple/audit-suppressions.sh` provably never sets the sentinel; only `*.test.sh`
sets it in its hermetic subshell. ts-filter derived-path override gated identically (prod filter
ALWAYS reads the repo-root generated `.pnpm-audit-ignore.json`).

**B. EXACT-match sentinel, fail-closed (operations refinement #1).** Gate on exactly
`[[ "${DEVLOOP_TEST:-}" == "1" ]]` — **NOT** `[[ -n "$DEVLOOP_TEST" ]]`. A truthy-loose check lets
`DEVLOOP_TEST=0`/`=false` enable overrides (attacker sets ANY value and wins). AND the fail-loud RED
must fire when the sentinel is present-but-not-exactly-`1`, not only when absent — else fail-loud
inherits the same truthy bypass. So: **override env present + sentinel != "1" (absent OR wrong value)
⇒ `STATUS=FAIL REASON=suppression-override-without-test-sentinel`** (the 5th REASON token, §B). A bare
override in a non-test env trips the gate RED, never silently dropped.

**C. CI-sentinel-leak runtime assertion (operations refinement #2 — LOAD-BEARING).** A+B trust that
production never sets the sentinel; C ENFORCES it at runtime. The always-run path runs inside
`layer-all.sh` in CI — if `DEVLOOP_TEST` ever leaks into the CI job env (careless workflow edit,
copied test step, a reusable action that sets it), the production check would honor overrides
repo-wide and silently, collapsing the whole property. So add a runtime assertion EARLY in
`layer-all.sh` (and/or `layer3.sh`): **HARD-FAIL if `GITHUB_ACTIONS=true` AND `DEVLOOP_TEST` is set
(ANY value) → `STATUS=FAIL REASON=test-sentinel-set-in-ci`** (the 6th REASON token, §B). This catches
the leak at the pipeline boundary. A static grep of `ci.yml` for `DEVLOOP_TEST` is cheap extra
defense-in-depth, but the RUNTIME assertion is the enforced one.

**Why all three:** A makes overrides off-by-default (fail-safe inversion); B removes the truthy
bypass in both the gate AND the fail-loud; C removes the residual "sentinel leaks into CI" hole that
A+B alone can't (A+B trust production never sets it; C enforces at runtime). Net property: the
always-run security check is **INERT to the ambient environment, provable by reading the check + the
`layer-all` assertion alone** — robust to any future caller or CI config. Cost: one string compare,
one runtime guard, two REASON tokens.

**Gate-2/3 (security, replaces the weaker "test envs unreachable on guard path"):** (1) override read
gated on EXACT sentinel `== "1"` (verifiable by reading the check script); (2) override-without-VALID-
sentinel reds the gate (absent AND wrong-value) with `suppression-override-without-test-sentinel`;
(3) ts-filter path gated identically; (4) the CI-leak assertion fires on `GITHUB_ACTIONS && DEVLOOP_TEST`
set → `test-sentinel-set-in-ci`; (5) production guard never sets the sentinel.

This is the operational teeth behind OC1: a green guard on a clean manifest proves nothing about the
FAIL path; J-a + J-b prove the FAIL path reaches CI's exit code. Confirm exact assertion shape with
@test.

### K. Docs deliverables (contributor doc + runbook + TODO mirror)

- **`docs/contributor/audit-suppressions.md`** (deliverable 5 + operations OC5): when to suppress vs
  fix; the **90-day default duration** stated explicitly; the renewal workflow written as a REAL,
  end-to-end-followable sequence — *edit `expires` in `audit-suppressions.toml` → run
  `scripts/audit-suppressions-check.sh --fix` → commit BOTH the manifest AND the regenerated derived
  files (`.cargo/audit.toml`, `.pnpm-audit-ignore.json`)*; how the always-run date-check enforces it;
  the fail-closed posture. @operations verifies followability at review.
  **+ §F.2 Dependabot reconciliation section (Lead "Both" ruling, @security-blessed):** state
  prominently that **`audit-suppressions.toml` is the ONLY sanctioned place to suppress an advisory**;
  **Dependabot is for bump PRs ONLY** — its alert-dismissal / `ignore:` MUST NOT be used as a
  suppression channel (that would fragment the SSOT); and the **manual enablement caveat** —
  committing `dependabot.yml` enables version-updates, but Dependabot *security-updates* require a
  repo Settings toggle a maintainer must flip (cannot be committed). @security verifies this section's
  wording at Gate 3 (it's the human-facing half of the second-surface reconciliation).
- **`docs/runbooks/devloop-validation.md`** (operations OC3 + @security/@operations co-confirmed,
  satisfies C3 + C4 together) — author in ONE pass, three concrete pieces:
  1. **Distinct REASON tokens** (NOT generic `guards-failed`) so on-call greps to the right row and
     the expiry-fire is an auditable security event. The check emits one per failure mode (these are
     also the §B emitted tokens — see §B):
     - past-due → `STATUS=FAIL REASON=suppression-past-due`
     - sync drift (manifest ↔ derived mismatch) → `REASON=suppression-drift`
     - malformed/parse error (manifest or derived) → `REASON=suppression-malformed`
     - quality-check fail (empty reason/ticket, bad expires/ecosystem) → `REASON=suppression-quality`
     - test-injection override env present without valid `DEVLOOP_TEST=1` sentinel →
       `REASON=suppression-override-without-test-sentinel` (security trust-boundary, §J — the runbook
       row notes this is an UNEXPECTED env-injection signal: a non-test environment set an override;
       investigate the CI config / for tampering, do NOT just unset-and-rerun)
     - test sentinel leaked into CI (`GITHUB_ACTIONS` + `DEVLOOP_TEST` set) →
       `REASON=test-sentinel-set-in-ci` (gate-integrity failure at the `layer-all`/`layer3` boundary,
       §J/C — the runbook row notes: the CI env has the test sentinel set, which would let production
       honor overrides repo-wide; find what set `DEVLOOP_TEST` in the workflow and remove it, do NOT
       unset-and-rerun blindly — treat as a pipeline-integrity incident)
     Add each to the **§6.3 keyed failure table** (L288 Layer-3 section) AND the **consolidated
     failure-index table** (~L465-468, the first thing on-call scans) with a §6.3 pointer.
  2. **§6.3 red-on-expiry note**: it's INTENDED, not an outage/flake. Action is NOT bypass — renew
     `expires` (after re-running the renewal verification, e.g. `cargo tree -p rsa --invert` for
     RUSTSEC-2023-0071) OR fix the advisory; pointer → `docs/contributor/audit-suppressions.md`. Note
     red-on-expiry is the FIRST signal (no warn-ahead window in #47) so teams renew proactively per
     the contributor-doc cadence.
  3. **L365 RECONCILIATION — SECURITY-REVIEWED, land verbatim/near-verbatim** (security verifies the
     exact wording at Gate 3; loose wording erodes the §11 ownership boundary). The existing L365
     reminder ("Operators should not modify allowlists or suppression flags as part of failure
     triage — escalate to security") would make on-call refuse to renew. Append @security's exact
     exception:
     > **Exception — suppression renewal on expiry:** when a Layer-3 `suppression-past-due` failure
     > fires, editing `audit-suppressions.toml` to renew the `expires` date (then
     > `scripts/audit-suppressions-check.sh --fix` to regenerate derived files) IS the sanctioned
     > remediation — NOT the prohibited ad-hoc allowlist edit. The prohibition targets SILENT,
     > incident-time suppression of a LIVE advisory via CLI flags or hand-edited derived files.
     > Renewal flows through the tracked manifest, regenerates derived files deterministically, and
     > lands as a reviewed commit. Security ownership (ADR-0033 §11) is preserved: the renewed
     > `reason`/`expires` MUST go through normal PR review — security reviews the renewal
     > justification (e.g. the re-run `cargo tree -p rsa --invert` build-time-only re-verification for
     > RUSTSEC-2023-0071). An operator MUST NOT extend an `expires` date as part of live incident
     > triage to make CI green; that remains prohibited and escalates to security.

     Load-bearing distinction: RENEWAL = reviewed manifest commit with re-verified justification
     (sanctioned); AD-HOC SILENCING = CLI flag / hand-edited derived file / unreviewed
     expires-bump-to-unblock-CI (prohibited, escalate). Both touch "suppressions"; only renewal
     preserves §11.
- **`docs/TODO.md`** (deliverable 6): "Suppressed Advisories" section mirroring the manifest; UPDATE
  the existing Cluster C 2026-08-08 reference → **2026-09-01** with a one-line "clock re-anchored to
  manifest-landing date" note (per @security decision A).
- **`docs/TODO.md` — glob-runner spin-out TODO (@test condition 1):** add an entry (Code Quality or
  a Validation-Pipeline section) explicitly naming the orphaned `*.test.sh` meta-tests that no
  layer/CI runs as a command — `scripts/lang/_common.test.sh`, `_dispatch.test.sh`,
  `_get_base_ref.test.sh`, `_get_base_ref.behavior-equivalence.test.sh`, `_layer_skeleton.test.sh`,
  `rust/ts/changed.test.sh` (proto's runs via layer-3 predicate meta-test; rust
  `behavior-equivalence.test.sh` runs only via `scripts/test.sh`'s doc-path) — and propose a general
  `find -name '*.test.sh'` glob-runner (a separate infra task: must decide hermeticity/budget/STATUS
  emission for the whole class). This makes @test's verdict RESOLVED-DEFERRED (one accepted spin-out
  they cite). **Condition 2 (does NOT weaken #47):** the explicit `layer3.sh` wire-in of
  `audit-suppressions-check.test.sh` stays regardless of whether the glob-runner ever lands — #47's
  own test runs every devloop + CI independent of the deferred general runner.
- **`docs/TODO.md` — suppression-expiry early-warning TODO (@security policy call: DEFER):** file
  under the suppression/observability-debt area, @security's framing verbatim: *"Suppression expiry
  early-warning (EXPIRING_SOON nudge) — DEFERRED from #47. Optional, non-posture (hard `expires` FAIL
  is the enforcement). Best built as a fast-follow once the §F scheduled scan lands, so the nudge
  rides the scheduled job / digest rather than per-PR log noise. Owner: observability + security.
  Depends-on: §F drift-catcher."* The `Depends-on: §F` line is load-bearing — it records that an
  EXPIRING_SOON line emitted only on ad-hoc pipeline runs misfires (the suppression OWNER rarely
  sees it), so a future devloop must NOT build it standalone. Mechanism if/when built: a distinct
  greppable `EXPIRING_SOON=<id>:<days_left>` line, NEVER `STATUS=WARN` (§6 has no WARN member; the
  hard `expires` FAIL stays the only mechanical teeth). @security explicitly distinguishes this
  (genuinely optional, non-posture) from the §F drift-catcher (hard co-sign condition) — do not
  conflate.

### Consolidated deliverables (updated — was 6, now 10 artifacts after Lead "Both" ruling)

1. `audit-suppressions.toml` (repo root) — seeded with the security-authored RUSTSEC-2023-0071 entry
   (§A.1). **[security policy content]**
2. `scripts/audit-suppressions-check.sh` — date/sync/quality/parse checks (SIX distinct
   `suppression-*`/`test-sentinel-*` REASON tokens) + `--fix` + sentinel-gated test-injection envs
   (A+B+C, §J: `AUDIT_SUPPRESSIONS_NOW`, `DEVLOOP_SUPPRESSIONS_MANIFEST` read only under exact
   `DEVLOOP_TEST=1`) + single `parse_suppressions` (§B, §I, §J).
3. Generated `.cargo/audit.toml` + `.pnpm-audit-ignore.json` (§A.2). **[security policy content]**
4. Conditional gate in `scripts/lang/{rust,ts}/audit.sh` — SKIPPED-NO-DIFF on no-dep-change, fail-
   closed tri-state, suppression filtering (split fail-mode §D.1.2), preserved+extended
   no-pass-through comments, conservative-skip comment (§D, §D.1). **[Minor-judgment security]**
5. Layer-3 guard `scripts/guards/simple/audit-suppressions.sh` (§E) + the CI-sentinel-leak runtime
   assertion in `scripts/layer-all.sh`/`layer3.sh` (§J/C) + self-test wire-in in `layer3.sh`.
   **[always-run wiring]**
6. **NEW** `.github/workflows/audit-scheduled.yml` — WEEKLY scheduled unconditional full scan via
   `DEVLOOP_AUDIT_FORCE_RUN=1` (force-run-only) gate-bypass, + §F.1 deduped-`audit-drift`-issue
   surfacing on failure via `actions/github-script` (§F, OC2).
7. **NEW (Lead "Both" ruling)** `.github/dependabot.yml` — cargo + npm/pnpm, weekly, NO `ignore:`,
   manifest-is-SSOT comment header (§F.2). **[second-suppression-surface policy = security's call]**
8. `scripts/audit-suppressions-check.test.sh` — table-driven self-test (consumes `_test_helpers.sh`
   + new `assert_status`/`assert_exit`), wired into `layer3.sh`; full §I matrix incl. A+B+C
   sentinel-gating, FAIL-branch fixtures, PATH-1 denial-of-coverage, OC1/§J layer3-non-zero.
9. Docs: `docs/contributor/audit-suppressions.md` (+ §F.2 SSOT / no-Dependabot-dismissal / manual
   security-updates-toggle notes), `docs/runbooks/devloop-validation.md` §6.3 (6 token rows + L365
   verbatim exception), `docs/TODO.md` "Suppressed Advisories" mirror + 2 spin-out TODOs (glob-runner,
   EXPIRING_SOON) (§K).
10. The **ADR-0033 §3 amendment** (§G) in `docs/decisions/adr-0033-polyglot-validation-pipeline.md`.

### Open questions for Gate 1
1. ✅ RESOLVED (@security A): expiry **2026-09-01 governs** (90-day clock re-anchors to manifest-
   landing date). TODO.md 2026-08-08 mirror gets updated to 2026-09-01 + re-anchor note.
2. ✅ RESOLVED (@security, final): **one-line** `reason` string (verbatim) + full rationale as a
   `#`-comment block above the entry + verbatim in `.cargo/audit.toml` (mandatory) + TODO long-form
   (§A.1). NOTE: this REVISED the earlier "full block in `reason`" draft → DROPS the multiline-parser
   requirement (parser is now flat `key="value"` only). Heads-up sent to @code-reviewer/@dry-reviewer.
3. ✅ RESOLVED (@security + @operations): both **bless the §3 reclassification**. @security on
   C1–C3 (§G.1); @operations on OC1–OC5 (§G.2). All conditions are folded into the design.
4. ✅ RESOLVED (@operations): **NO explicit ci.yml step** (router-drift / ADR-0033 §10) — rely on
   Layer 3 via layer-all; PROVE it fires via OC1/§J (§E).
5. ✅ FULLY RESOLVED (@operations + @security): **§F = WEEKLY scheduled full-scan, IN #47**
   (`audit-scheduled.yml`, `DEVLOOP_AUDIT_FORCE_RUN=1` gate-bypass). Operations chose weekly (≪ §12
   14-day MTTR); dependabot fallback NOT taken. No longer a Lead pick — both reviewers made it a hard
   co-sign condition. (§F/§G.3.)
6. ✅ RESOLVED (@security): derived-file formats confirmed (§A.2/§C); **ts dep-manifest gate set
   finalized** incl. `pnpm-workspace.yaml` (§D). **Malformed-file fail direction ADJUDICATED
   (§D.1.2):** SPLIT — filter fails SAFE (apply zero suppressions, don't abort scan); always-run
   check fails LOUD (hard-fail). My earlier fail-loud-filter lean is overridden by security's split.
7. ✅ RESOLVED (@observability): **`SKIPPED-NO-DIFF` over N/A** confirmed; plus the new required
   `SUPPRESSED=<ids>` applied-suppression line (§D). Early-warning deferred (no STATUS=WARN).
8. ⏳ @dry-reviewer + @code-reviewer: confirm gate placement **(D-b)** — gate inside `audit.sh` via
   shared helpers, keep dispatcher untouched (§D). The multiline-`reason` tokenizer is **NO LONGER
   NEEDED** (q2 resolution → flat parser); just confirm the simpler flat parser + single-parse-helper.

---

## Pre-Work
Lead surveyed existing audit machinery (`scripts/lang/{rust,ts}/audit.sh`, `scripts/audit.sh`,
`_changed_helpers.sh` `diff_touches_path`/`diff_touches_root_files`, `ci.yml`) and the
security-authored RUSTSEC-2023-0071 policy block in `docs/TODO.md`. None of the target config files
exist yet. No code committed in pre-work.

---

## Gate 2 — Validation Log

**Env note**: `dt-guard` built locally + run with `DT_GUARD=target/debug/dt-guard` (binary not
pre-built in clone). `shellcheck` is NOT installable in this sandbox (no root) — `bash -n` is clean
on all 9 new/changed scripts; **shellcheck MUST run in CI** (carried as a known gap, flagged by the
implementer and Lead). Layer 7 env-tests = `wave2-pending` N/A (harness not implemented).

### Attempt 1 — FAIL (Layer 3, 2 guard findings)
- `validate-cross-boundary-scope`: plan table didn't match the diff (4 inbound files missing as
  discrete rows; `ci.yml` listed but correctly not touched; a compound row) → routed to implementer.
- `validate-todo-tracking`: **pre-existing** inline-debt-body at `2026-06-03-.../main.md:297` (task
  #46 doc, committed at 63ca0fb; not this diff). Fixed by Lead.

### Inter-attempt remediation
- Lead removed the offending parenthetical from task #46's §Accepted Deferrals and committed it
  **standalone** (`2949a1b`) — both to clear `todo-tracking` and to avoid the
  `cross-boundary-multi-devloop-collision-2-main-mds` failure (two modified devloop main.mds).
- Implementer corrected the Cross-Boundary table (discrete rows; removed `ci.yml`).

### Attempt 2 — PASS
- L1 compile / L2 fmt / L5 lint: SKIPPED-NO-DIFF (no rust/ts/proto source changed).
- **L3 guards: 32/32 PASS** (incl. the new `audit-suppressions` guard).
- L4 test: shell self-test `audit-suppressions-check.test.sh` **38/38** (wired into layer3); existing
  meta-tests pass; rust/ts test SKIPPED-NO-DIFF.
- **L6 audit: OK** — conditional gating validated LIVE: rust ran + `SUPPRESSED=RUSTSEC-2023-0071` +
  `cargo-audit-passed`; ts `SKIPPED-NO-DIFF no-dep-changes`; `buf breaking` OK.
- L7 env-tests: N/A (`wave2-pending`).
- Artifact-specific: `*.sh` → shellcheck (could-not-run, CI gap); no proto/migration/k8s/Dockerfile.

**Gate 2 result: PASS** (with the shellcheck-in-CI caveat). Advancing to review.

---

## Gate 3 — Reviewer Verdicts (running)

| Reviewer | Verdict | Notes |
|----------|---------|-------|
| Security (paired) | **RESOLVED-FIXED** | F4 re-verified (ran test, 45/45); full carry-list verified; 2 Approved-Cross-Boundary trailers provided |
| Test | **RESOLVED-DEFERRED** | F1 test-rot + F2 _audit_gate.sh tri-state (new _audit_gate.test.sh 18/18 both clocks, scanner-not-invoked proof) both FIXED + re-verified; 1 accepted spin-out (glob-runner → docs/TODO.md) |
| Observability | **CLEAR** | Verified live: SKIPPED-NO-DIFF non-masking, SUPPRESSED= line, alertable past-due msg, 7 tokens, always-run backstop observable |
| Code Quality | **RESOLVED-FIXED** | 4 findings fixed (DRY dup, dead doc anchor, python3-dep ADR disposition, filter fail-safe test); ADR-0033 §3 amendment verified (supersedes rejected alt); Gate-3 gating item: confirm 2 security trailers on commit; shellcheck=CI gap |
| DRY | **RESOLVED-FIXED** | 2 true-dup findings fixed in-PR (shared read_pnpm_ignore_ids lib + assert_no_ci_sentinel_leak in _common.sh); both Gate-1 watch-points held; 0 deferred |
| Operations | **CLEAR** | OC1–OC5 + test-seam + §F BOTH verified E2E (ran past-due fixture → guard reds layer3); §3 joint co-sign stands; net posture does not weaken |
| Semantic Guard | **CLEAR** (native SAFE) | Fail-closed verified: parser non-zero on malformed, §D.1.2 split correct, tri-state skip fails closed, `|| true` is NOT fail-open (STATUS=FAIL still dominates→exit 1); no creds |


**Gate 3 result: PASS** (no ESCALATED). 3× CLEAR (Observability, Operations, Semantic Guard[SAFE]) +
3× RESOLVED-FIXED (Security, Code Quality, DRY) + 1× RESOLVED-DEFERRED (Test). Review took 2
iterations: round 1 surfaced DRY ×2 + code-reviewer F2/F3/F4; round 2 surfaced test F1 (test-rot) +
F2 (audit_gate tri-state untested, caught when the implementer conflated it with the F4 filter).
All findings fixed in-PR; one accepted spin-out (glob-runner).

---

## Accepted Deferrals

- `docs/TODO.md` §`*.test.sh` glob-runner — general shell self-test harness (Test spin-out)
- `docs/TODO.md` PATH-1 fail-safe e2e (real-pnpm) — function-level floor in place; e2e deferred (code-reviewer-accepted)

---

## Lessons Learned
1. **F4 ≠ F2**: the implementer conflated the ts-filter fail-safe (code-reviewer's F4) with the
   `audit_gate` tri-state matrix (test's F2). Lead grep of the test suite caught that one of three
   "fixed" findings was actually only two — the gate (the RUN/SKIP decision that IS the feature) was
   still untested. Verify findings against the actual code, not the implementer's summary.
2. **Devloop-doc edits post-dating the last guard run slip the gate**: task #46's §Accepted
   Deferrals inline-debt-body (committed at 63ca0fb) failed validate-todo-tracking here because that
   doc was finalized after task #46's final guard run. Run the doc-discipline guards AFTER the doc is
   final.
3. **Pre-existing-blocker cleanups need their own commit**: editing task #46's main.md to clear a
   pre-existing guard failure created a two-active-devloop collision; committing it standalone
   (2949a1b) kept #47's diff to a single active devloop.
4. **shellcheck couldn't run in-sandbox** (no root) — `bash -n` clean on all 12 scripts is syntax,
   not lint; the shellcheck gate must run in CI. Carried as a known coverage gap (all reviewers concur).
