# Devloop Output: Audit Advisory Cleanup (Task #48)

**Date**: 2026-06-07
**Task**: Clear known-fixable advisories surfaced post-PR-58-merge — pnpm audit (vitest >=4.1.0 workspace-wide, nx/tmp advisories) + cargo audit (findings beyond RUSTSEC-2023-0071); truly unfixable residue lands in `audit-suppressions.toml` with expires/reason/ticket.
**Specialist**: infrastructure (paired with security)
**Mode**: Agent Teams (v2)
**Branch**: `feature/browser-client-join-task-48`
**Duration**: 2026-06-07 → 2026-06-08 (plan → Gate 1 all-7-confirmed → implementation → Gate 2 (2 attempts: squash + rider + Lead L4 override) → Gate 3 ALL CLEAR). Final commit `e871297` (single-commit convention).

**Final summary**: pnpm audit 3→0 (vitest 4.1.8 critical fix + tmp 0.2.7/brace-expansion 5.0.6 in-range lockfile-only); cargo audit clean with the security-approved RUSTSEC-2025-0052 noise-only suppression (expires 2026-09-05); latent #47 derived-file generator bug fixed; TODO.md async-std entry corrected + suppression mirror row; one team-lead-approved test rider (auth_tests camelCase) + 27 further pre-existing GC camelCase failures accepted by Lead L4 override and spun out to a global-controller devloop. Zero production Rust, Cargo.lock unchanged.

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `a03f2915e84fb6429c301c1b05de9e5b9d829523` |
| Branch | `feature/browser-client-join-task-48` |

---

## Loop State (Internal)

<!-- This section is maintained by the Lead for state recovery after interruption. -->
<!-- Do not edit manually - the Lead updates this as the loop progresses. -->

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer@devloop-task48-audit-cleanup` |
| Implementing Specialist | `infrastructure` |
| Iteration | `2` final (Gate 1 OK; Gate 2 attempt 1 FAIL→fixed: L3 scope-guard saw only HEAD^..HEAD of a 3-commit split, squashed; L4 gc auth_tests rider; attempt 2: L4 RED on 27 pre-existing GC camelCase failures accepted by Lead override + spun out; Gate 3 ALL CLEAR, all 7 verdicts) |
| Security (paired) | `security@devloop-task48-audit-cleanup` |
| Test | `test@devloop-task48-audit-cleanup` |
| Observability | `observability@devloop-task48-audit-cleanup` |
| Code Quality | `code-reviewer@devloop-task48-audit-cleanup` |
| DRY | `dry-reviewer@devloop-task48-audit-cleanup` |
| Operations | `operations@devloop-task48-audit-cleanup` |
| Semantic Guard | `semantic-guard@devloop-task48-audit-cleanup` |

### Gate 1 Plan Confirmations

| Reviewer | Plan Status |
|----------|-------------|
| Security (paired) | confirmed (Domain-judgment co-author role engaged; RUSTSEC-2025-0052 suppression APPROVED with wording conditions: warning-class/non-gating explicit, verify-command + fail-closed clause, expires 2026-09-05, ticket → corrected TODO entry; will review final entry text during implementation) |
| Test | confirmed (Minor-judgment owner ACK granted for `packages/test-utils/package.json`, conditions: `Approved-Cross-Boundary: test` commit trailer; post-bump run-summary evidence in main.md vs 6-file/14-test baseline; ≥1 run via `nx run-many -t test:unit`) |
| Observability | confirmed (TODO.md Minor-judgment co-owner ACK granted; 0.29+ correction independently verified; non-blocking: fix 2 more stale fragments in same TODO entry — 0.25-era companion-bump list + pre-#47 `audit-config.toml` filename) |
| Code Quality | confirmed (non-blocking notes: stale `audit-config.toml` filename in TODO line; add classification row if vitest.config.ts needs edits; Gate-3 trailer reminders) |
| DRY | confirmed (nit folded into planned TODO.md edit: stale "audit-config.toml" filename in async-std entry) |
| Operations | confirmed (TODO.md Minor-judgment co-owner pre-ACK; supports SUPPRESS per 2026-05-25 pre-authorization; 4 build criteria: renewal playbook in manifest comment, resolvable ticket anchor, TODO stale-fragment fixes, Gate-2 evidence = pnpm audit 0 + layer6 SUPPRESSED= lines + suppressions-check green + frozen-lockfile install) |
| Semantic Guard | confirmed (condition: any emergent test-file/vitest.config.ts or production-source edits must be added to the Classification table before Gate 3) |

---

## Task Overview

### Objective
Clear known-fixable dependency advisories surfaced after PR-58 merged:

1. **pnpm audit** — bump vitest workspace-wide to >=4.1.0 (closes critical CVE; includes test-utils that landed via task #8); resolve nx/tmp advisories via bumps where safe. Run all TS test suites after each bump to validate.
2. **cargo audit** — investigate findings beyond RUSTSEC-2023-0071 (already seeded by #47); bump where safe.
3. Any residue truly unfixable (no upstream patch, no safe workaround) lands in #47's `audit-suppressions.toml` with proper expires (default 90 days from now → 2026-09-05), explicit reason, and ticket pointer.

**Out of scope**: building the suppression machinery itself (task #47); RUSTSEC-2023-0071 (seeded by #47); major-version bumps unrelated to advisories.

Story: task #48 in `docs/user-stories/2026-05-02-browser-client-join.md`

### Scope
- **Service(s)**: Workspace-wide dependency manifests (TS workspace + Rust workspace)
- **Schema**: No
- **Cross-cutting**: Yes — dependency bumps affect all services; suppression file affects Layer 6 audit gate

### Debate Decision
NOT NEEDED — mechanical advisory cleanup with predefined suppression policy from task #47.

---

## Cross-Boundary Classification

Per ADR-0024 §6. None of the planned paths fall inside a Guarded Shared Area (§6.4 enumerated list checked; `audit-suppressions.toml` is not enumerated — its policy ownership is handled via the §6.5 Paired flag, security is paired on this devloop).

| Path | Classification | Owner (if not mine) |
|------|----------------|---------------------|
| `packages/test-utils/package.json` | Minor-judgment — major-version bump of the test framework (`vitest` ^3.2.0 → ^4.1.8) in a test-owned package; compat pre-verified, validated by running the full suite. Owner confirmation needed at Gate 1 + Gate 3. | test |
| `pnpm-lock.yaml` | Mine (dependency hygiene = infrastructure) / Mechanical | — |
| `audit-suppressions.toml` | Domain-judgment — suppression reason/expires is security-owned (ADR-0033 §11). Handled via §6.5 Paired flag: security is paired on this devloop; the entry is co-authored with @security before finalizing. | security (paired) |
| `.cargo/audit.toml` | Mine / Mechanical — GENERATED from the manifest via `scripts/audit-suppressions-check.sh --fix`, committed alongside per #47 sync-check. | — |
| `docs/TODO.md` | Minor-judgment — add a row to the "Suppressed Advisories" mirror table + correct a factually wrong resolution note ("bump to 0.25+" → "0.29+") in the async-std entry owned by observability + operations (both are Gate-1 reviewers here). | observability + operations |
| `docs/devloop-outputs/2026-06-07-audit-advisory-cleanup-task48/main.md` | Mine | — |
| `scripts/audit-suppressions-check.sh` | Mine (EMERGENT, added during implementation per semantic-guard Gate-1 condition) — suppression machinery is infrastructure-owned per the manifest header ("Policy ownership: security. Machinery: infrastructure."). 1-line generator fix: the derived-file expires annotation hardcoded the rsa-specific verify command for every entry; latent #47 bug first manifested by this task's second entry. Not policy (no change to what is suppressed or how); meta-test 48/48 green. | — (policy untouched; security informed) |
| `docs/contributor/audit-suppressions.md` | Mine (EMERGENT, security-finding follow-through) — the doc carried the SAME expiry-day off-by-one that security's sign-off required fixing in the manifest playbook ("CI goes red on the expiry day" vs the actual strictly-past-due `__date_check` semantics). 1-paragraph factual correction so the renewal-workflow doc the playbook points to doesn't contradict the playbook at renewal time. No workflow/policy change. | — (factual fix per security's own finding) |
| `crates/gc-service/tests/auth_tests.rs` | Not mine, Mechanical (EMERGENT, team-lead-approved rider at Gate 2 attempt 1) — 1-line camelCase assertion alignment (`body["service_type"]` → `body["serviceType"]`) with the R-53-locked wire shape, which is pinned by the handler's own unit test (`handlers/me.rs:94`). STRENGTHENS the test (was asserting against a key that no longer exists → Null). Pre-existing failure, NOT introduced by this loop — see §Issues Encountered item 4. | global-controller (no reviewer on this loop — surfaced via this row + §Issues note + commit-message body for story-level PR review) |

Not in the table because not in the diff: `.pnpm-audit-ignore.json` was regenerated by `--fix` and came back byte-identical (no js suppressions), so it is intentionally absent both here and from the commit — the scope guard requires the planned set to equal the touched set. The no-diff outcome is itself recorded as evidence in §Devloop Verification Steps.

---

## Planning

### Observed findings (enumerated 2026-06-07, this branch, post-#47)

**pnpm audit — 3 advisories:**

1. **CRITICAL GHSA-5xrq-8626-4rwp** — `vitest` <4.1.0 (installed 3.2.4 via `packages/test-utils`, the workspace's only vitest consumer). Vitest UI server arbitrary file read/execute.
   - **Fix**: bump devDep `vitest` `^3.2.0` → `^4.1.8` (latest; >=4.1.0 satisfied).
   - **Compat pre-verified**: vitest@4.1.8 peers `vite ^6.0.0 || ^7.0.0 || ^8.0.0` (we have vite 6.4.2), `@types/node ^20 || ^22 || >=24` (we have 22.10.5), engines node ^20/^22/>=24 (workspace pins >=22 <23). `vitest.config.ts` uses plain node environment, no workspace file, no v4-removed options; the `coverage` block names provider v8 but coverage is never invoked (`vitest run`, and `@vitest/coverage-v8` was never installed — behavior unchanged).

2. **HIGH GHSA-ph9p-34f9-6g65** — `tmp` 0.2.5 <0.2.6 (path traversal), via `nx>tmp`. nx declares `tmp: ~0.2.1`, so 0.2.6 is **in-range**.
   - **Fix**: lockfile-only `pnpm update tmp` → 0.2.6. No manifest change, no override needed.

3. **MODERATE GHSA-jxxr-4gwj-5jf2** — `brace-expansion` 5.0.5 (>=5.0.0 <5.0.6), via `nx>minimatch@10.2.3` which declares `^5.0.2`, so 5.0.6 is **in-range**.
   - Below the ts-audit gate threshold (gate is `--audit-level=high`) but trivially fixable: lockfile-only `pnpm update brace-expansion` → >=5.0.6.

**cargo audit — 1 finding beyond RUSTSEC-2023-0071, non-gating:**

- **RUSTSEC-2025-0052** — `async-std` 1.13.2 unmaintained (discontinued 2025-08-24). *Warning*, not a vulnerability; `cargo audit` exits 0 ("1 allowed warning found") — it does NOT fail Layer 6.
  - **Reachability** (verified this session): pulled ONLY via `opentelemetry_sdk 0.24.1`'s `testing` feature (`testing → rt-async-std → async-std`), enabled by `crates/common`'s **dev-dependency** for `InMemorySpanExporter`. `cargo tree -i async-std -e normal` is **empty** → async-std is never compiled into shipped binaries; it exists only in the test build graph + Cargo.lock.
  - **Fixability** (verified against the crates.io index): the `testing` feature carries `rt-async-std` through every release up to and including 0.28.0; the async-std dependency is gone only at **opentelemetry_sdk 0.29.0+**. So the fix is an OTel-stack migration 0.24 → 0.29+ (five breaking releases across `opentelemetry`/`opentelemetry_sdk`/`opentelemetry-otlp`/`tracing-opentelemetry`, plus `crates/common` telemetry-init API churn) — already tracked as its own cross-cutting work item in `docs/TODO.md` (async-std entry, owner: observability + operations, P3). **Not a safe bump within this task** (major-version migration; the only in-scope-adjacent part is that it's advisory-related, but the migration cost puts it squarely in the tracked follow-up).
  - NOTE: that TODO entry's resolution line says "bump the OTel stack to 0.25+" — **factually wrong** (0.25–0.28 still pull async-std); will correct to 0.29+ with the registry evidence.

### Plan

1. **vitest bump** — edit `packages/test-utils/package.json` devDep to `^4.1.8`; `pnpm install`; run full TS suites (`pnpm lint`, `pnpm test:unit`, `pnpm build` via nx run-many).
2. **tmp + brace-expansion** — in-range lockfile refresh (`pnpm update tmp brace-expansion`); re-run the TS suites (these invocations exercise nx itself, the sole consumer).
3. **Verify** — `pnpm audit` expect 0 advisories.
4. **RUSTSEC-2025-0052 suppression** (decision point, worked with @security — RESOLVED 2026-06-07: security explicitly updated their earlier no-entry position to APPROVED with 4 wording conditions, citing the operations pre-authorization, the review-noise condition firing, ID-scoping, and no stale-ID wedge risk; both positions and the reconciliation are preserved in the message log): add a manifest entry —
   - `id = "RUSTSEC-2025-0052"`, `ecosystem = "rust"`, `expires = "2026-09-05"` (default 90d from 2026-06-07), `ticket` → the `docs/TODO.md` async-std/OTel-bump entry, `reason` = dev/test-graph-only via opentelemetry_sdk 0.24 `testing` feature; verify command of record `cargo tree -i async-std -e normal` (must be empty); fail-closed if a normal-graph consumer ever appears; fix is the tracked OTel 0.29+ migration.
   - Full exposure analysis in a `#`-comment block above the entry per `docs/contributor/audit-suppressions.md`.
   - Regenerate derived files: `scripts/audit-suppressions-check.sh --fix`; run the check read-only to verify green; commit manifest + derived files together.
   - **Honest framing for security**: this warning does not gate (cargo-audit warnings are non-fatal), so the alternative is "leave it visible". Recommendation to suppress rests on (a) this story arc (#46–48) being explicitly audit-noise cleanup, and (b) the existing TODO entry pre-authorizing exactly this: "Add to … ignore-with-justification entry if the warning becomes review-noise before the bump devloop lands." Security has the final call; if security prefers leave-visible, steps 4–5 reduce to the TODO correction only.
5. **docs/TODO.md** — add the RUSTSEC-2025-0052 row to the "Suppressed Advisories" mirror table; correct the async-std entry's resolution version (0.25+ → 0.29+) and note the suppression; also fix the entry's stale `audit-config.toml` filename → `audit-suppressions.toml` (DRY + observability + code-quality Gate-1 nit — the manifest landed under the latter name in #47) and the 0.25-era companion-bump list (`opentelemetry-otlp 0.17→0.18` + `tracing-opentelemetry 0.25→0.26`) which is stale alongside the 0.29+ correction (observability Gate-1 nit).
6. **Record** implementation summary + verification evidence here.

**No Rust code or `Cargo.lock` changes** — the cargo side is manifest/suppression-only.

### Out of scope (confirmed against observed findings)
- OTel stack 0.24 → 0.29+ migration (tracked TODO, observability + operations).
- nx major bump (nx 20.3.0 advisories resolve in-range; no major needed).
- RUSTSEC-2023-0071 (seeded by #47, untouched).

---

## Pre-Work

None

---

## Implementation Summary

### pnpm side (steps 1–3) — DONE

1. **vitest 3.2.4 → 4.1.8** (`packages/test-utils/package.json` devDep `^3.2.0` → `^4.1.8`): closes CRITICAL GHSA-5xrq-8626-4rwp. No config/source changes needed — `vitest.config.ts` and all test files untouched (semantic-guard condition: no emergent edits outside the planned set).
2. **tmp 0.2.5 → 0.2.7 + brace-expansion 5.0.5 → 5.0.6** (closes HIGH GHSA-ph9p-34f9-6g65 + MODERATE GHSA-jxxr-4gwj-5jf2): lockfile-only via `pnpm update -r --depth Infinity tmp brace-expansion` (plain `pnpm update <pkg>` is a silent no-op for transitive deps — the `-r --depth Infinity` form is required). Side effect: brace-expansion@2.1.0 → 2.1.1 (in-major patch within minimatch@9's `^2` range — v1/v2 consumers NOT forced across majors; verified in the lockfile diff: `minimatch@9.0.9 → brace-expansion: 2.1.1`).
   **DECISION (security-accepted at implementation, recorded so the next audit devloop doesn't re-litigate): lockfile-only was the deliberate choice over `pnpm.overrides`.** Rationale: both patched versions are in-range of the declaring packages (nx declares `tmp: ~0.2.1`; minimatch@10.2.3 declares `brace-expansion: ^5.0.2`), pnpm resolves highest-in-range on refresh, so overrides would be dead config the moment they land. Regression coverage without the override ratchet: tmp is HIGH-severity, so any future re-resolution below 0.2.6 re-fails the dep-change-gated TS audit; brace-expansion is moderate (below the high gate) — residual regression exposure accepted by security, with the weekly forced scan (`DEVLOOP_AUDIT_FORCE_RUN=1`, `audit-scheduled.yml`) as the backstop.
3. **Supply-chain summary of the lockfile diff** (for security Gate-3): net −7 packages (+16/−23). One genuinely new package name: `obug@2.1.2` (vitest 4's debug replacement, vitest dependency family). In-family majors: chai 5.3.3→6.2.2, es-module-lexer 1.7.0→2.1.0, std-env 3.10→4.1, tinyexec 0.3.2→1.2.4, tinyrainbow 2→3. Dropped vitest-3 internals: vite-node, tinypool, tinyspy, cac, check-error, deep-eql, loupe, pathval, strip-literal, js-tokens.

### cargo side (steps 4–5) — DONE (final entry text with @security for pre-commit review)

- **RUSTSEC-2025-0052 suppression entry applied** to `audit-suppressions.toml` after security's explicit approval (updating their earlier no-entry position; rationale: operations pre-authorization + review-noise condition firing + ID-scoped + no stale-ID wedge). Entry satisfies all 4 security wording conditions (warning-class/non-gating explicit in reason AND comment; verify-command of record + fail-closed no-grace-period clause; expires 2026-09-05 with sunset terms; ticket → corrected TODO entry) plus operations' renewal playbook and the ID-SCOPED clause from security's rationale.
- **Derived files regenerated** via `scripts/audit-suppressions-check.sh --fix`: `.cargo/audit.toml` ignore list now `["RUSTSEC-2023-0071", "RUSTSEC-2025-0052"]` with mirrored rationale comments; `.pnpm-audit-ignore.json` unchanged (no js suppressions).
- **Emergent machinery fix** (`scripts/audit-suppressions-check.sh:410`): the derived-file generator hardcoded the rsa-specific verify text (`cargo tree -p rsa --invert`) into EVERY entry's expires annotation — a latent #47 bug that only manifests with a second entry, which this task is the first to add. Without the fix, the generated `.cargo/audit.toml` would tell a 3am reader to run the rsa verify command for the async-std advisory. Fixed to an entry-agnostic line (the entry-specific verify command already lives in each mirrored reason line); machinery is infrastructure-owned per the manifest header ("Machinery: infrastructure"). Meta-test suite: 48 passed, 0 failed. NOT a scope violation of "building the suppression machinery itself is out of scope" — this repairs a defect the in-scope deliverable triggers, with the smallest possible diff.
- `docs/TODO.md` async-std entry corrected: resolution 0.25+ → **0.29+** (registry-verified: `testing` feature carries `rt-async-std` through opentelemetry_sdk 0.28.0; 0.29-era pairings named per observability), stale companion-bump list corrected, stale `audit-config.toml` filename → `audit-suppressions.toml`, 2026-06-07 re-verification recorded, suppression status + 2026-09-05 expiry noted. Mirror-table row added (summary = faithful condensation of the manifest reason line per DRY).

---

## Files Modified

| File | Change |
|------|--------|
| `packages/test-utils/package.json` | vitest devDep `^3.2.0` → `^4.1.8` (Minor-judgment, owner test — Gate-1 ACK granted) |
| `pnpm-lock.yaml` | vitest 4.1.8 re-resolution + tmp 0.2.7 + brace-expansion 5.0.6/2.1.1 |
| `docs/TODO.md` | async-std entry: 0.29+ correction + filename fix + companion-list fix + re-verification + suppression status; mirror-table row added |
| `audit-suppressions.toml` | RUSTSEC-2025-0052 entry (security-approved, expires 2026-09-05) |
| `.cargo/audit.toml` | regenerated via `--fix` (ignore list + mirrored rationale) |
| `scripts/audit-suppressions-check.sh` | 1-line generator fix: entry-agnostic expires annotation (was hardcoded rsa-specific for all entries) + explanatory comment |
| `docs/contributor/audit-suppressions.md` | expiry-semantics correction (valid THROUGH `expires`, red the day after — strictly-past-due), matching security's required playbook fix |
| `crates/gc-service/tests/auth_tests.rs` | 1-line rider (team-lead-approved, Gate 2): stale `service_type` → `serviceType` assertion, pre-existing from 92d963b (see §Issues item 4) |
| `docs/user-stories/2026-05-02-browser-client-join.md` | Step-9 tracking: task #48 → Completed + Devloop Output path; new spin-out row #49 (GC camelCase test migration, global-controller). Scope-guard-exempt (whole-file user-story tracking carve-out); no classification row required. |
| `docs/devloop-outputs/2026-06-07-audit-advisory-cleanup-task48/main.md` | this record |

---

## Devloop Verification Steps

### TS suite evidence (test Gate-1 condition: vs 6-file/14-test baseline)

- After vitest bump (`pnpm run test:unit` = `nx run-many -t test:unit`):
  `RUN v4.1.8 /work/packages/test-utils — Test Files 6 passed (6), Tests 14 passed (14), Duration 140ms` — exact baseline match, no skips, no test-source edits. `pnpm run lint` (tsc --noEmit, 2 projects) green; `pnpm run build` (vite lib build + dts) green.
- After tmp/brace-expansion lockfile refresh: re-ran `pnpm run test:unit` via nx run-many — `Test Files 6 passed (6), Tests 14 passed (14)`; lint + build green (nx cache hit = same inputs hash post-refresh); `pnpm exec nx affected -t test:unit --base=a03f2915` resolves targets and passes (nx target discovery intact after its transitive deps moved).
- Lockfile churn scope (test Gate-1 condition 3): every changed `pnpm-lock.yaml` entry verified to belong to the vitest dependency family (vitest, vite-node→removed, chai 5→6, check-error/deep-eql/loupe/pathval→removed-as-chai-5-internals, es-module-lexer 1→2, std-env 3→4, tinyexec 0.3→1.2, tinyrainbow 2→3, tinypool/tinyspy/cac/strip-literal/js-tokens→removed, convert-source-map/obug→added) plus `tmp` 0.2.5→0.2.7 and `brace-expansion` 5.0.5→5.0.6 + 2.1.0→2.1.1. NO unrelated peer re-resolution: vite stays 6.4.2, @types/node stays 22.10.5, nx subtree otherwise untouched.
- Commit trailers (ADR-0024 §6.7, one per Minor-judgment row, per code-reviewer Gate-3 mandatory verdict items):
  - `Approved-Cross-Boundary: test vitest 3->4 major bump ACKed at Gate 1, suite green 6 files/14 tests` (exact text per test's owner ACK)
  - `Approved-Cross-Boundary: observability TODO.md async-std entry corrections (0.29+ fix, stale filename + companion-list) co-owner-ACKed at Gate 1`
  - `Approved-Cross-Boundary: operations TODO.md async-std entry corrections (0.29+ fix, stale filename + companion-list) co-owner-ACKed at Gate 1`
  - (one trailer line per owner per the review-protocol.md convention of record, per code-reviewer)

### Audit evidence

- `pnpm audit` → "No known vulnerabilities found" (all 3 advisories cleared, including the moderate below the gate threshold).
- `pnpm audit --audit-level=high` → "No known vulnerabilities found".
- `pnpm install --frozen-lockfile` → clean (operations Gate-2 criterion).
- Cargo side (post-suppression, operations Gate-2 criteria):
  - `scripts/audit-suppressions-check.sh --fix` → regenerated + `STATUS=OK REASON=suppressions-clean`; read-only re-run → `STATUS=OK REASON=suppressions-clean`.
  - Machinery meta-test `scripts/audit-suppressions-check.test.sh` → **48 passed, 0 failed** (covers the emergent generator fix).
  - `cargo audit` → exit 0, no warning output (RUSTSEC-2025-0052 suppressed via tracked derived file).
  - `scripts/layer6.sh` final run: `SUPPRESSED=RUSTSEC-2023-0071,RUSTSEC-2025-0052` (exactly the manifest-tracked set), `STATUS=OK REASON=cargo-audit-passed`, `STATUS=OK REASON=pnpm-audit-passed`, `STATUS=OK REASON=audit-all-langs-ok`, `STATUS=OK REASON=buf-breaking-passed`, `LAYER=6 RESULT=OK`.
  - **Post-sign-off wording fix evidence** (security's one required fix: expiry-day off-by-one in the renewal playbook — entry valid THROUGH 2026-09-05, red from 2026-09-06, strictly-past-due `__date_check` semantics): fix applied to the manifest playbook + the same pre-existing error corrected in `docs/contributor/audit-suppressions.md`; `--fix` re-run (derived files identical — comment blocks aren't mirrored), suppressions-check read-only `STATUS=OK REASON=suppressions-clean`, layer6 re-run `SUPPRESSED=RUSTSEC-2023-0071,RUSTSEC-2025-0052` all `STATUS=OK`, `.pnpm-audit-ignore.json` confirmed still empty-ignore (`"ignore": []`).
  - **Layer 3 always-run guard path** (operations Start-Review ask — the surface the new entry's expiry will fire through): `scripts/guards/simple/audit-suppressions.sh` → `STATUS=OK REASON=suppressions-clean`, rc=0, run post-0052-entry with both commits in place.
- `Cargo.lock`: UNCHANGED (asserted via `git diff --stat` — no Rust dependency moved).

### Full `./scripts/layer-all.sh` summary (Gate 2 attempt 2, at single commit)

| Layer | Result | Notes |
|-------|--------|-------|
| 1 | OK | |
| 2 | OK | |
| 3 | OK | scope guard no-drift (after squash); 32/32 guards; meta-tests green |
| 4 | **FAIL** | **Pre-existing GC camelCase test debt, ACCEPTED BY LEAD OVERRIDE** (see below + §Issues item 6). NOT this devloop's regression. |
| 5 | OK | |
| 6 | OK | `SUPPRESSED=RUSTSEC-2023-0071,RUSTSEC-2025-0052` |
| 7 | N/A | wave2-pending |

`TOTAL_RESULT=FAIL` — solely on the Layer 4 pre-existing debt. **This devloop's own validation is GREEN**: L1/2/3/5/6 OK, and the one L4 delta attributable to this devloop (the auth_tests rider) is 15/15. The 27 remaining L4 failures are GC-domain debt that predates the start commit (byte-identical at `a03f2915`, verified via pristine worktree), accepted by explicit Lead override at Gate 2 (zero production Rust in this diff; Cargo.lock unchanged) and spun out to its own global-controller devloop. See §Issues item 6 and §Accepted Deferrals.

---

## Code Review Results

**Gate 3: ALL CLEAR — all 7 verdicts, no escalations.**

| Reviewer | Verdict | Notes |
|----------|---------|-------|
| Security (paired) | RESOLVED-FIXED | 1 found / 1 fixed / 0 deferred — the expiry-day off-by-one in the renewal playbook (entry valid THROUGH expires, red the day after); fixed in the manifest + the contributor doc that carried the same pre-existing error. Suppression entry sign-off; supply-chain lockfile review clean (obug provenance verified). |
| Test | CLEAR | Owner ACK on the vitest bump (6 files/14 tests exact baseline, ≥1 nx run-many run); rider verified pre-existing + strictly-strengthening; Mechanical classification accepted (no upgrade — owner confirmation structurally unavailable, zero added coverage). |
| Observability | CLEAR | TODO.md async-std corrections verified (0.29+ registry evidence, 0.29-era pairings, filename, re-verification note). |
| Code Quality | CLEAR | Trailer convention (one line per owner) applied; no-config-change confirmed; diffs from a03f2915. |
| DRY | CLEAR | No true-duplication findings; generator-template fix noted as a DRY win. Logged 1 extraction opportunity (reqwest x6 / uuid drift) — informational per ADR-0019, NOT a finding; folded into this commit. |
| Operations | CLEAR | 4 build criteria + 3 generator-fix Start-Review checks satisfied; renewal playbook + Layer 3 guard-path evidence accepted. |
| Semantic Guard | CLEAR | Both emergent edits classified before Gate 3; no production source touched; credential-leak look on the generator line clean. |

Gate-1 condition threads all closed pre-commit; Gate-2 resolutions (squash, rider, Lead L4 override) recorded in §Issues items 4–6. No reviewer deferrals.

---

## Accepted Deferrals

**Pre-existing GC camelCase test debt — Layer 4, accepted by Lead override at Gate 2 (2026-06-07).** 27 pre-existing GC integration-test failures (`meeting_create_tests.rs` 7/13, `meeting_tests.rs` 19/38, `gc-test-utils/src/server_harness.rs:214` 1/7) are the GC mirror of task #46: task #23/R-53 (`92d963b`) migrated GC's wire structs to camelCase but never updated the DB-gated `#[sqlx::test]` integration tests. Verified byte-identical at start commit `a03f2915` (pristine worktree) — NOT introduced by this devloop, whose diff has zero production Rust and an unchanged Cargo.lock. **Deferred to a to-be-scheduled global-controller devloop** (GC implementer; per-field wire-shape judgment is GC domain — #46 Path A kept some OAuth fields snake_case). Tracking record: `docs/TODO.md` §Test Debt "GC integration tests not migrated to camelCase wire shape — the GC mirror of task #46". Full enumeration + evidence in §Issues item 6.

**DRY extraction opportunity (informational, ADR-0019 — NOT a finding, NOT a scope deferral):** pre-existing `reqwest` verbatim-pin across 6 Cargo.tomls (not in `[workspace.dependencies]`) + `uuid` 1.10/1.11 drift in env-tests. This loop touches no Cargo.toml, so it's out of scope here; logged to `docs/TODO.md` §Cross-Service Duplication "From DRY Reviewer (Ongoing)" for the next reqwest/uuid-touching change (append folded into this commit).

Note: the Layer 4 GC test debt above was a **Lead Gate-2 override**, not a reviewer deferral — no reviewer punted scope; the Lead accepted a pre-existing gate failure that is byte-identical at the start commit and unrelated to this diff, and spun it out to its own devloop.

---

## Rollback Procedure

If this devloop needs to be reverted:
1. Verify start commit from Loop Metadata: `a03f2915e84fb6429c301c1b05de9e5b9d829523`
2. Review all changes: `git diff a03f2915..HEAD`
3. Soft reset (preserves changes): `git reset --soft a03f2915`
4. Hard reset (clean revert): `git reset --hard a03f2915`
5. Dependency bumps: lockfile changes revert cleanly with git reset; no migrations or infra applies expected.

---

## Issues Encountered & Resolutions

1. **`pnpm update tmp brace-expansion` was a silent no-op for transitive deps** (pnpm 10 named-update semantics only touch direct deps). Resolution: `pnpm update -r --depth Infinity tmp brace-expansion`. Caught immediately by re-grepping the lockfile (and flagged in advance by both security and operations); the `pnpm audit = 0` arbiter would also have caught it.
2. **#47 generator bug surfaced by the second suppression entry**: `scripts/audit-suppressions-check.sh:410` hardcoded the rsa-specific verify command (`cargo tree -p rsa --invert`) into every entry's expires annotation in the derived `.cargo/audit.toml` — the 0052 rationale block told readers to run the wrong command. Resolution: 1-line entry-agnostic fix (per-entry verify command already lives in each mirrored reason line); meta-test 48/48 green; classified as an emergent Mine/infrastructure-machinery edit, flagged to security + team-lead + operations.
3. **Security position conflict**: security's pre-plan direct message said NO-suppression while the Gate-1 record said APPROVED-with-conditions (messages crossed in flight). Resolution: explicit reconciliation requested and received — approval stands, pre-plan position superseded; both positions preserved in the message log and the plan's step-4 marker updated from OPEN to RESOLVED.
4. **Gate 2 attempt 1 — Layer 4 pre-existing failure + approved rider fix** (`crates/gc-service/tests/auth_tests.rs:257`, `test_me_endpoint_with_valid_token`): NOT introduced by this loop — verified by running the test at the start commit `a03f2915` in a pristine temp worktree (failed identically, Null vs "global-controller"). Origin: commit `92d963b` ("camelCase wire-format migration (R-53, task #23)") migrated the GC `/me` handler to camelCase (`serviceType`, pinned by the handler's own unit test at `handlers/me.rs:94`) but missed this one integration-test assertion, which still read `body["service_type"]`. Identical regression class to the AC fix in task #46 (`f5fc4b4`). Team-lead approved fix-in-this-devloop (1-line test-only, no design ambiguity, blocks Layer 4 regardless); classified Mechanical, owner global-controller (no reviewer on this loop — THIS note + the classification row + the commit-message body are the GC owner's visibility at story-level PR review; deliberately not a silent rider). Verified the only camelCase-sensitive key in the file; the other 14 tests in the file pass.
5. **Gate 2 attempt 1 — Layer 3 scope-guard failure**: the guard evaluates "THIS edit only" (working-tree-vs-HEAD when dirty, else HEAD^..HEAD), so the loop's 3-commit structure (implementation + security fix + evidence) made HEAD look like a main.md-only edit with 7 planned-untouched violations. Resolution: squashed to a single commit on the start commit per the devloop single-commit convention; also absorbed the Lead's concurrent main.md Loop-State edit (dirty tree shrinks the guard scope) and removed the `.pnpm-audit-ignore.json` classification row (regeneration was byte-identical → planned-but-untouched violation; replaced with a prose note under the table).
6. **Gate 2 attempt 2 — Layer 4 RED on 27 pre-existing GC test failures, accepted by Lead override + spun out.** Fixing the auth_tests rider (item 4) let cargo's fail-fast advance past `auth_tests` and expose the rest of the same debt class: `crates/gc-service/tests/meeting_create_tests.rs` (7/13 fail), `crates/gc-service/tests/meeting_tests.rs` (19/38 fail), `crates/gc-test-utils/src/server_harness.rs:214` (1/7 fail) — 27 failures total, all the GC mirror of task #46. Root cause: task #23/R-53 (`92d963b`) migrated GC's `models/mod.rs` request/response structs to `#[serde(rename_all="camelCase")]` (several `deny_unknown_fields`) but updated NO GC integration tests; they are DB-gated `#[sqlx::test]`, so the task-#23 clone never ran them (the false-CLEAR mechanism #46 documented). Verified byte-identical at start commit `a03f2915` via pristine worktree — NOT introduced by task #48 (zero production Rust in the diff; Cargo.lock unchanged). Resolution: **explicit Lead override of the L4 gate at Gate 2**, debt documented here + `docs/TODO.md` §Test Debt and deferred to a to-be-scheduled global-controller devloop (per-field wire-shape judgment + the #46 wire-shape lock pattern are GC domain). Not absorbed as a rider: scale (27 across 3 files, request+response sides) + domain judgment (#46 Path A kept some OAuth fields snake_case) + no GC reviewer on this loop. See §Accepted Deferrals.

---

## Lessons Learned

1. **Enumerate advisories before planning remediation channels.** Both the override-vs-lockfile and suppress-vs-fix debates collapsed once the declared ranges (`tmp ~0.2.1`, `brace-expansion ^5.0.2`) and the gate behavior (cargo-audit warnings are non-fatal) were observed facts rather than assumptions.
2. **Registry-verify "bump to fix" claims in tracking docs.** The TODO entry's "OTel 0.25+" resolution was wrong (rt-async-std persists through opentelemetry_sdk 0.28.0); an upgrade devloop scoped to 0.25 would have burned an iteration discovering it. The crates.io index check that caught this took one command.
3. **Generators with a single config entry hide entry-shape bugs.** The #47 derived-file generator looked correct for 14 months of one-entry operation; the second entry exposed the hardcoded text. When reviewing generator code, ask "what does the output look like with N=2".
4. **Crossed teammate messages on a domain-owned decision need explicit reconciliation, not inference.** Holding the suppression step until security restated their standing position cost little and kept the ADR-0033 §11 ownership chain clean.
