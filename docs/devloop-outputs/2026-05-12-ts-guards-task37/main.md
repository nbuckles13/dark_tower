# Devloop Output: TS guards under `scripts/guards/simple/ts/` (Task #37)

**Date**: 2026-05-12
**Task**: Land six TS guards (R-62, ADR-0033 Wave 2 #6): `no-secrets-in-ts`, `no-pii-in-logs-ts`, `no-test-removal-ts`, `name-guard-dt-client` (R-26), `no-dev-trust-path-in-prod-bundle` (R-14), `exports-map-closed`.
**Specialist**: client (paired with security)
**Mode**: Agent Teams (v2) — full
**Branch**: `feature/browser-client-join-task37`
**Duration**: TBD

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `91cf097ad33367b3b5dee29326af0cf0af42c7a6` |
| Branch | `feature/browser-client-join-task37` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | client |
| Implementing Specialist | client |
| Iteration | 1 |
| Security (paired) | RESOLVED |
| Test | RESOLVED |
| Observability | RESOLVED |
| Code Quality | APPROVE |
| DRY | PASS |
| Operations | APPROVE |

---

## Task Overview

### Objective
Land six TypeScript-targeted simple guards under `scripts/guards/simple/ts/` to close the TS-side gap in the polyglot validation pipeline (ADR-0033 Wave 2 #6). Each guard self-classifies via path glob and exits 0 cleanly when no relevant files are present (target packages like `sdk-core`, `web-app` do not yet exist — guards are scaffold-now, fire-later).

### Scope
- **Service(s)**: tooling / `scripts/guards/simple/ts/`
- **Schema**: No
- **Cross-cutting**: Yes — touches conventions owned by security, observability, test, plus client-owned R-14/R-26 enforcement

### Debate Decision
NOT NEEDED — ADR-0033 (`docs/decisions/adr-0033-polyglot-validation-pipeline.md`) already specifies the six guards in §Wave 2 #6 and assigns owners (line 404–410). The 2026-05-06 polyglot-pipeline debate decided the policy; this devloop is execution.

### Open question — RESOLVED in planning
- `no-dev-trust-path-in-prod-bundle.sh` (renamed from `bundle-content-r14.sh` for self-documenting clarity): **Decision: forcing-function guard now, Vitest contract test when task #9 lands `packages/sdk-core/`**. The guard mechanically fails when `sdk-core` lands without its bundle-content contract test, eliminating the "forget to write the test" failure mode. Full rationale in §Planning Q1. Final decision for this devloop.

---

## Cross-Boundary Classification

To be filled by implementer in plan. Each guard names a convention or rule owned by a specific specialist; the guard implementation itself is in client's domain (shell pattern detection), but the **rule definition** is the owner's:

| Path | Classification | Owner (if not mine) |
|------|----------------|---------------------|
| `scripts/guards/simple/ts/no-secrets-in-ts.sh` | Mechanical | security |
| `scripts/guards/simple/ts/no-pii-in-logs-ts.sh` | Mechanical | observability |
| `scripts/guards/simple/ts/no-test-removal-ts.sh` | Mechanical | test |
| `scripts/guards/simple/ts/name-guard-dt-client.sh` | Mine | — |
| `scripts/guards/simple/ts/no-dev-trust-path-in-prod-bundle.sh` | Mine | — |
| `scripts/guards/simple/ts/exports-map-closed.sh` | Not mine, Minor-judgment | security |
| `scripts/guards/run-guards.sh` | Not mine, Minor-judgment | operations |
| `docs/decisions/adr-0033-polyglot-validation-pipeline.md` | Mechanical | operations |
| `docs/TODO.md` | Mechanical | operations |

**Classification rationale**:
- **`no-secrets-in-ts`, `no-pii-in-logs-ts`, `no-test-removal-ts`**: Mechanical (ADR-0024 §6.2). Each is a sed-test-clean port of an existing Rust guard with the same logic in different language regex. The pattern set (PII fields, secret variable names, test-block keywords) is identical to the Rust equivalents — no new rule shape, no new policy. Reviewer scope: confirm the regex is faithful to the Rust analog.
- **`name-guard-dt-client`**: Mine. R-26 / R-24 are client-owned client telemetry conventions; the metric prefix `dt_client_*` is defined in the client section of the browser-client-join user story.
- **`no-dev-trust-path-in-prod-bundle`**: Mine. R-14 governs the prod-bundle exclusion of the dev-only WebTransport fingerprint path — pure client-side build/runtime concern. Self-documenting name describes the invariant (no dev-only trust-path in prod bundle), not the requirement number.
- **`exports-map-closed`**: Not mine, Minor-judgment. The "closed-world public surface" policy is security's (ADR-0028 §5 supply chain). The choice of test-only path globs (`./test-only/*`, `./test/*`, `./internal/*`) and the `private: true`/`@darktower/test-` allowlist mechanism are judgment calls that need security's sign-off, not just a mechanical mirror.
- **`run-guards.sh` recursion**: Not mine, Minor-judgment. The line 99 ADR comment says "KEEP — universal, runs all simple/*.sh" — the spirit of that is "discovery is universal, per-guard self-classification preserved." Switching to `find ... | sort` with explicit fixtures prune preserves the spirit and is co-signed by @operations.
- **`docs/decisions/adr-0033-polyglot-validation-pipeline.md`**: Mechanical, owner: operations. Per @operations: fold the line-99 wording update into this devloop so the ADR text matches the new runner behavior. Line 99 currently reads `KEEP — universal, runs all simple/*.sh (per-guard self-classification preserved)`; change to `KEEP — universal, runs all simple/**/*.sh recursively, fixtures pruned (per-guard self-classification preserved)`. Operations co-signed this as mechanical text-update. No other ADR sections change.
- **`docs/TODO.md`**: Mechanical, owner: operations (canonical owner of the forward-looking TODO tracker). Per @team-lead direction: add a §R-14 Transition entry naming the canonical test path `packages/sdk-core/tests/bundle-content.test.ts` so #9's implementer has a single source of truth for the obligation the guard mechanically enforces. The entry text is parallel to the §Tech Debt #1 entry in this main.md.

**Notes for implementer**:
- None of the listed paths are in Guarded Shared Areas (ADR-0024 §6.4). GSA list covers `proto/**`, `crates/common/src/{jwt,meeting_token,token_manager,secret}.rs`, auth/crypto in AC, `crates/common/src/webtransport/**`, `db/migrations/**`, `crates/media-protocol/**`, `crates/ac-service/src/audit/**`, ADR-0027 primitives — none of which this task touches.
- Pattern-detection logic that mirrors an existing Rust guard (e.g., `no-pii-in-logs.sh` Rust → `no-pii-in-logs-ts.sh` TS-equivalent) is typically **Mechanical** under §6.2 (sed-test clean: same logic, different language regex). Each row's classification should be defended explicitly.
- `exports-map-closed.sh`: the rule ("`exports` is closed-world; no test-only modules in the prod exports map") is security's. The implementation (parse package.json, walk subpaths) is client's. Default classification: **Not mine, Minor-judgment** unless the implementer argues it is mechanical.

---

## Constraints

1. **`run-guards.sh` recursion** (resolved in §Planning Q2): extend via `shopt -s globstar` + `simple/**/*.sh` glob. Co-signed by @operations conditional on minimal diff. Operations is owner of the change row.
2. **Guards self-classify and exit 0 when no TS files have changed**, mirroring the Rust-guard pattern (`get_modified_files` returns empty → print "no Rust files changed" → exit 0). Use `scripts/lang/ts/changed.sh` for the gate test, OR replicate the same logic with `get_modified_files "$SEARCH_PATH" ".ts"` / `".tsx"` / `".svelte"` / `package.json`.
3. **Diff base honors `GUARD_DIFF_BASE`** (set by CI for PRs), falls back to `HEAD` locally — use `get_diff_base` from `common.sh`. Do not hardcode `HEAD` or `main`.
4. **No target packages exist yet** for `name-guard-dt-client` and `no-dev-trust-path-in-prod-bundle`: `packages/` contains only `proto-gen` and `test-utils`. `name-guard-dt-client` exits 0 cleanly when target packages are absent. `no-dev-trust-path-in-prod-bundle` exits 0 today (state 1: `packages/sdk-core/` absent) but MECHANICALLY FAILS the moment `sdk-core` lands without its contract test (state 2: forcing function). See §Planning Q1.
5. **Match the existing simple-guard pattern**: `set -euo pipefail`, source `common.sh`, `init_violations` / `increment_violations` / `print_*` helpers, structured output with `VIOLATION:` markers (run-guards.sh greps for `VIOLATION|ERROR` on failure).
6. **TS guard search paths MUST exclude generated/installed/build directories**: `node_modules/`, `dist/`, `build/`, `.svelte-kit/`, `coverage/`. (Per @operations.) Each grep-based TS guard applies this exclusion to its file list before scanning. Implementation: a single shared exclude predicate at the top of each guard, e.g. `grep -Ev '/(node_modules|dist|build|\.svelte-kit|coverage)/'` applied to the file list from `get_modified_files`. The exclusion is in the file-list filter, not after grep — keeps the scan cheap.

---

## R-14 / R-26 Quick Reference

**R-14 (line 78 of user story)**: `BrowserWebTransport` real implementation. Dev-only `serverCertificateHashes` path gated behind build-time literal `__DEV_TRUST_FINGERPRINT__` (false in prod, tree-shaken out). **CI bundle-inspection assertion verifies prod build excludes `serverCertificateHashes` string.** The Vitest contract test (lands with #9) checks the built artifact; `no-dev-trust-path-in-prod-bundle.sh` is the forcing function ensuring the test exists.

**R-26 (line 115 of user story)**: Client-side bounded `event` enum + structured logs. Metric naming convention is `dt_client_*` (per R-24, line 104). `name-guard-dt-client.sh` rejects metric names in TS source that don't match `dt_client_*`. **Scope-clarification**: R-26 governs *structured logs* on the client (browser console / GC `/api/v1/telemetry`). It does NOT govern the server-side `auth_events` audit table on AC (unchanged per ADR-0020). The TS guard targets `packages/**` only.

---

## Planning

### Open question resolutions

**Q1 — R-14 enforcement: guard vs. Vitest contract test?** Decision: **forcing-function guard now (renamed `no-dev-trust-path-in-prod-bundle.sh`), Vitest contract test once `packages/sdk-core/` lands in task #9**, with the guard mechanically forcing the transition.

**Naming**: renamed from `bundle-content-r14.sh` → `no-dev-trust-path-in-prod-bundle.sh` for self-documenting clarity. Compare to `no-hardcoded-secrets`, `no-pii-in-logs`, `validate-alert-rules`: they describe the rule, not the requirement number. R-number references force every future reader to grep the user-story. The new name says what the guard enforces — no dev-only trust path in the production bundle.

**Why a forcing-function guard, not a passive placeholder**:
- The real check is "the prod bundle excludes `serverCertificateHashes`," which requires a `vite build --mode production` artifact. Until `sdk-core` exists, no artifact to inspect — a Vitest contract test cannot be written.
- The R-14 rule must be in place ahead of #9 landing (ADR-0033 §Wave 2 #6 sequences before the consumer packages).
- Passive placeholder ("exit 0 until sdk-core appears, then someone remembers to write the test") opens a memorial-channel failure mode: the day sdk-core lands, the contract test could simply be forgotten. The forcing function eliminates that mode by failing CI mechanically.
- Once `sdk-core` exists, the most faithful check is `vite build` + grep the emitted bundle. That belongs as a Vitest contract test under `packages/sdk-core/tests/bundle-content.test.ts` because (a) shares the prod build pipeline with other bundle assertions, (b) running `vite build` from a shell guard would blow the Layer-3 latency budget, (c) the test runs in the same CI environment that produces the artifact. **The guard cannot reproduce this check well; the test must exist.**

**Forcing-function logic (per @team-lead/user discussion)** — four states implemented by §Per-guard design #5:
1. `packages/sdk-core/` does not exist → exit 0 (rule scaffolded, no consumer yet).
2. `packages/sdk-core/` exists BUT `packages/sdk-core/tests/bundle-content.test.ts` does NOT exist → **FAIL** with a message telling #9's implementer exactly where to land the test. This is the forcing function.
3. Both `sdk-core/` and the canonical test path exist → exit 0 (Vitest contract test in Layer 4 now carries the real check).
4. (Belt-and-suspenders) if `packages/sdk-core/dist/` also exists, grep for `serverCertificateHashes` — fail on hit. Cheap Layer-3 catch for stale-build cases in PR-author trees.

**Hardcoded path coupling**: the canonical test path `packages/sdk-core/tests/bundle-content.test.ts` is named NOW in this plan, in the guard's header comment, and in `docs/TODO.md` §R-14 Transition (added by this devloop). #9's implementer reads the failure message + the TODO entry, lands the test at the canonical path. If they choose a different path, the guard's expected path is a one-line edit in the same devloop. **No Nx wiring required** (per @test item 5): `scripts/lang/ts/test.sh:10` already runs `nx affected -t test:unit test:component`; precedent `packages/test-utils/project.json`.

**Lifecycle after #9 lands**: with the Vitest test carrying the real check, this guard becomes redundant. Two options for #9's devloop to choose: (a) delete the guard, OR (b) keep it as a stale-`dist` tripwire (state 4 only). Recorded in §Tech Debt / Deferrals #1.

**Why this is testable today via ad-hoc proof**: all four states can be exercised by synthetic dir creation (state 1: no `packages/sdk-core/`; state 2: synthetic empty dir; state 3: synthetic dir + test file; state 4: synthetic dist/ with the literal). No sdk-core stub needed in the repo, no toolchain choices pulled forward, no scope drift.

**Q2 — `run-guards.sh` discovery strategy.** Decision: **extend `run-guards.sh` to recurse via `find ... | sort` with an explicit `*/fixtures/*` prune predicate** (per @operations' updated conditions in team-lead's consolidated must-fix list). Concrete shape:
```
while IFS= read -r guard; do
  ...
done < <(find "$SIMPLE_GUARDS_DIR" -type f -name "*.sh" -not -path '*/fixtures/*' | sort)
```
- Rationale: ADR-0033 §1.5 lays out subdirectories `simple/{rust,ts,proto,universal}/`. The `-not -path '*/fixtures/*'` prune predicate is operationally explicit — encodes the 2026-05-08 no-committed-fixtures policy as a structural invariant in the runner even though no fixtures land in this devloop, so a future accidental fixture addition does not auto-execute as a guard.
- Existing `simple/*.sh` continue to be discovered (depth 1).
- `sort` for deterministic ordering — output is stable across machines / filesystems.
- Operations classification: **Not mine, Minor-judgment**, owner: operations. Co-signed.
- This devloop ships only `simple/ts/` guards; rust/proto/universal subdirectories are migrated in their own devloops (out of scope here).

**Q3 — Fixtures?** Decision: **no committed fixtures** per `docs/TODO.md:341` (2026-05-08 policy, resolved via `devloop-outputs/2026-05-08-guard-self-test-cleanup`): *"new guards don't add fixtures — implementer proves correctness with ad-hoc scripts during the guard-authoring devloop, discarded before commit."* I'm acknowledging this policy is binding for this devloop; my initial draft proposed fixtures without acknowledging the policy, which was a mistake. Correctness for the six TS guards will be demonstrated via ad-hoc scripts during implementation, discarded before commit.
- If TS-guard fixtures genuinely merit reopening (because TS regex fragility differs structurally from Rust), that is a separate follow-up micro-debate, not a side effect of this devloop.
- Consequence: no `*/fixtures/*` exclude needed in `run-guards.sh` recursive discovery (simplifies the change to a clean `find ... -name "*.sh" -type f | sort`).

### Per-guard design

**1. `no-secrets-in-ts.sh`** (security domain; Mechanical port of `no-hardcoded-secrets.sh`; rule shape co-signed by @paired-security)
- **Trigger**: `get_modified_files "$SEARCH_PATH" ".ts"`, `.tsx`, `.svelte` — exit 0 if empty.
- **Check 1 — secret variable assignments** (per @paired-security S1(a), TS-narrowed identifier list to reduce false-positives on browser/SDK code where `token` is a routine runtime assignment): `(password|secret|api_key|credential|master_key|private_key|client_secret)\s*[:=]\s*["'\`][^"'\`]+`. **`token` is INTENTIONALLY DROPPED from Check 1** — real token leaks are caught by Check 2 (prefix patterns) and Check 3 (JWT shape).
  - Exclusions before pattern match: `process\.env\.|import\.meta\.env\.|Deno\.env\.get\(|Bun\.env\.|globalThis\.__VITE_DEFINE__|^\s*//|/\*` (per @paired-security S4: native, Deno, Bun, Vite build-time define, and comments).
- **Check 2 — API key prefixes** (identical to Rust): `"(sk-[a-zA-Z0-9]{20,}|pk-[a-zA-Z0-9]{20,}|AKIA[A-Z0-9]{16}|ghp_[a-zA-Z0-9]{36}|gho_[a-zA-Z0-9]{36}|xox[baprs]-[a-zA-Z0-9-]+)"`.
- **Check 3 — Connection strings with credentials** (per @paired-security S2, mirrors Rust Check 3 verbatim): `"(postgresql|mysql|redis|mongodb|amqp)://[^:]+:[^@{$]+@`. Closes a real coverage gap — TS code does construct connection strings (test setup, migration scripts, `pg.Pool` configs).
- **Check 4 — Authorization headers with tokens** (per @paired-security S3, NON-NEGOTIABLE; mirrors Rust Check 4 verbatim): `"(Authorization:\s*(Bearer|Basic)\s+[A-Za-z0-9+/=_.~-]{20,})"`. The most common form of a leaked credential in TS source.
- **Check 5 — JWT-like pattern**: `["'\`]eyJ[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}`.
- **Rust Check 5 (long-base64 heuristic) — DEFERRED** per @paired-security: TS false-positive rate is much higher (inlined source-maps, base64-encoded sprites, crypto-key fixtures). Captured in §Tech Debt / Deferrals #6.
- **Path filter**: drop matches in `**/*.test.ts`, `**/*.spec.ts`, `**/__tests__/**`, `**/test-utils/**`, `**/fixtures/**`. Plus Constraint #6 dir-list excludes.
- No `filter_test_code` (that helper invokes the Rust compiler). For TS, path-based filtering inline per-guard. **Per @dry-reviewer: do NOT pull up to `filter_test_code_ts` in `common.sh`.**

**2. `no-pii-in-logs-ts.sh`** (observability domain; Mechanical mirror of `no-pii-in-logs.sh`; rule defined by @observability)
- **Scope**: `packages/**/*.ts` AND `packages/**/*.svelte`. Exclude `node_modules`, `dist`, `*.d.ts`, plus the test-fixture paths below.
- **PII pattern (IDENTICAL to Rust, per @observability)**: `email|phone|phone_number|ip_address|ip_addr|user_agent|full_name|user_name|first_name|last_name|real_name`. **Do NOT extend** — per @observability, R-26 client-side join flow does not touch `address|dob|ssn|credit_card` and adding them would bloat false-positive surface without coverage gain. Re-evaluate when a future story introduces those fields.
- **Unified log-sink regex** (per @observability): `\b(console|logger|log)\.(log|info|warn|error|debug|trace|emit)\s*\(` — covers native `console.*`, the bounded-event `logger.*` landing in task #12, and the `@opentelemetry/api-logs` `logger.emit()` shape (caught by `emit` verb). **OUT of scope** per @observability F-OBS-2: OTel tracing-attribute sinks (`span.setAttribute(...)`, `span.setAttributes({...})`) are NOT matched by this regex. Deferred to a future Layer-4 semantic check (TS-AST-aware) once `sdk-core` starts emitting span attributes. Captured in §Tech Debt #9.
- **Check 1 (BLOCKING)** — PII identifier/property referenced inside a log call (`\b(${PII_PATTERNS})\b` on the same line as a log-sink call). Increments violation counter; exit 1 if non-zero.
- **Check 2 (BLOCKING)** — PII as a structured field/property in a log call's object argument (e.g., `{ email: ..., ip_address: ... }`). Single-line greedy match is acceptable for the simple-guard tier per @observability; multi-line semantic catch is left to semantic-guard. Increments violation counter; exit 1 if non-zero.
- **Check 3 (WARNING, non-blocking)** — PII in error messages (`throw new Error(...)`, `new Error(\`...\`)`, template literals). **Emits YELLOW warning only; does NOT increment the violation counter** (mirrors `no-pii-in-logs.sh` Check 4). Rationale: false-positive rate on legitimate descriptive errors is too high for hard-block.
- **Allow (must NOT flag)**:
  - Identifiers ending in `_hash` / `_id_hash` (e.g., `meeting_id_hash`, `user_id_hash`).
  - String literal `[REDACTED]` or comment markers `REDACTED|masked|hashed`.
  - Single-line comments (lines starting with `//`) — filter before pattern match.
  - **`// pii-safe: <reason>` opt-out marker** on the same line — exempt (per @observability).
  - R-26 sanctioned bounded-event fields (`event_type`, `client_version`, `trace_id`, `duration_ms`, `failure_stage`, `meeting_id_hash`, `org_id`, `mh_index_bucket`, `close_reason`, `outcome`, `status`) — **must never trigger**.
- **Invariant (per @observability MF-OBS-3b)**: by design, none of the R-26 sanctioned field names above match the PII pattern (`email|phone|phone_number|ip_address|ip_addr|user_agent|full_name|user_name|first_name|last_name|real_name`). **The PII pattern is closed-list and additive only — any future expansion MUST be cross-checked against this R-26 field list before merging. Owner: observability.** This invariant replaces the regression-fixture safety net that would normally guard regex changes (no committed fixtures per `docs/TODO.md:341`).
- **Test fixture exemption** (path-based, per @observability scope: this guard does NOT use the Rust `filter_test_code`): exclude `*.test.ts`, `*.spec.ts`, `**/test-utils/**`, `**/__tests__/**`, `**/__fixtures__/**`.

**3. `no-test-removal-ts.sh`** (test domain; Mechanical mirror in spirit of `test-coverage.sh`; final shape per @test rule-ownership)

**v1 scope: file-deletion-only.** The net-block-count heuristic is dropped for v1; deferred to a separate follow-up guard if/when warranted by an incident. Per @test (rule owner per ADR-0024 §6.2): block-counting across multi-line `it.each(`, `test.skip(`, template literals is brittle and erodes review-trust on false positives. Strict-cheap-v1 + defer-heuristic mirrors TODO.md:341 (2026-05-08 guard-self-test-cleanup) project pattern. Ruling reaffirmed by @team-lead.

- **File patterns tracked**: `packages/**/*.{test,spec}.{ts,tsx}` AND `packages/**/__tests__/**/*.{ts,tsx}` (the latter is already present — `packages/test-utils/src/__tests__/*.test.ts`). Regex also includes `.test.svelte` for forward-compat (no Svelte tests today).
- **Empty-diff idempotence**: if `get_modified_files` ∪ `get_deleted_files` contains zero TS-test files, exit 0 immediately before any other work. Required by ADR-0033 self-classification clause.
- **Check 1 (only check in v1) — deleted test files**: `get_deleted_files . ".test.ts"` and equivalents (`.spec.ts`, `.test.tsx`, `.spec.tsx`, plus `__tests__/`-located TS files). For each deleted test file, look for a matching addition (relaxed match per @test): same basename in additions, OR any new test file added in the same package directory. If neither, VIOLATION naming the deleted file.
- **Pathspec excludes** (per @operations Constraint #6 + @test): `node_modules/`, `dist/`, `build/`, `.svelte-kit/`, `coverage/`, `.nx/cache/` excluded from the file list before scanning.
- Deferred to follow-up guards (in §Tech Debt / Deferrals):
  - Net `(it|test)(` block-count heuristic — captured in §Tech Debt #2.
  - `.skip` / `.only` detection — captured in §Tech Debt #3.

**4. `name-guard-dt-client.sh`** (client domain; R-26 / R-24; ADR-0019 Pattern B — implementation is mine, **rule (regex, prefix, shape) is @observability's**)
- **Scope**: `packages/sdk-core/src/**/*.ts` AND `packages/web-app/src/**/*.ts`. Exit 0 cleanly when neither path exists. **`packages/test-utils/**` is exempt by package** — non-compliant names there are intentional per `packages/test-utils/src/InMemoryMetricsSink.ts:1-8` (deliberately passive recorder for testing the production sink's wrapping behavior; firing on test-utils would break the design intent). Also exclude `*.test.ts`, `*.spec.ts`, `**/__fixtures__/**`, `node_modules`, `dist`. `packages/sdk-svelte/` is NOT in observability's listed scope — DEFERRED to whenever a Meter lands there (captured in §Tech Debt #5).
- **Rule (per @observability)**: every metric name passed to an OTel `Meter` factory method MUST match `^dt_client_[a-z][a-z0-9_]{0,53}$`.
  - Literal prefix `dt_client_`
  - First char after prefix: lowercase letter (no leading digit/underscore)
  - Subsequent: `[a-z0-9_]` (snake_case, digits OK)
  - **No trailing underscore** — terminated by `$` after the char class.
  - Max total length 64 (Prometheus/OTel default) — encoded via `{0,53}` (prefix 9 + 54 ≤ 63 ≤ 64).
- **Meter API scanned (all seven factory methods, per @observability)**: `createCounter|createHistogram|createUpDownCounter|createGauge|createObservableCounter|createObservableGauge|createObservableUpDownCounter`. Concrete regex: `\.(createCounter|createHistogram|createUpDownCounter|createGauge|createObservable(Counter|Gauge|UpDownCounter))\s*\(\s*['"\`]([^'"\`]+)['"\`]` with capture group 2 = metric name.
- **Out of scope for this guard** (per @observability): label-name validation, R-25 metric-name coverage checks, and the runtime `MetricsSink` reject-bad-names enforcement (R-24). This guard is the static-lint layer only.
- **False-positive surface**: literal-first-arg calls only. Variable-name first args are skipped (out of scope; semantic-guard or runtime metric-coverage guards catch those).

**5. `no-dev-trust-path-in-prod-bundle.sh`** (client domain; R-14) — forcing-function per Q1

The guard mechanically enforces the R-14 transition timeline: the day `sdk-core` lands without its bundle-content contract test, CI fails. Four states, evaluated in order:

- **State 1 — no `packages/sdk-core/`** (today's state until task #9):
  prints `STATUS: no-dev-trust-path-in-prod-bundle — sdk-core not yet present (rule scaffolded; no consumer yet)`. Exit 0. The rule is in place ahead of #9 landing per ADR-0033 §Wave 2 #6 sequencing.

- **State 2 — `packages/sdk-core/` exists BUT `packages/sdk-core/tests/bundle-content.test.ts` does NOT exist** (the forcing function):
  prints
  ```
  VIOLATION: no-dev-trust-path-in-prod-bundle — R-14 enforcement gap.
  sdk-core has landed but its bundle-content contract test is missing.
  The canonical path is:
      packages/sdk-core/tests/bundle-content.test.ts
  Land the test alongside sdk-core, then this guard becomes a belt-and-
  suspenders check against the production bundle.
  See §R-14 Transition in docs/TODO.md.
  ```
  Exit 1. This is the load-bearing forcing function — it removes the "forgot to write the test" failure mode by failing CI mechanically.

- **State 3 — both `packages/sdk-core/` and `packages/sdk-core/tests/bundle-content.test.ts` exist**:
  prints `STATUS: no-dev-trust-path-in-prod-bundle — contract test present (Layer 4 carries the real check)`. Exit 0 (unless state 4 fires).

- **State 4 — belt-and-suspenders, in addition to state 3** (only evaluated if state 3 is true): if `packages/sdk-core/dist/` exists, grep it for `serverCertificateHashes`. On hit: `VIOLATION: no-dev-trust-path-in-prod-bundle — serverCertificateHashes literal found in <file>` and exit 1. On clean (or no dist): no message, exit 0. Cheap Layer-3 catch for stale-build cases in PR-author trees.

**Honesty note**: state 2 is the load-bearing forcing function; states 3 + 4 are belt-and-suspenders against an imperfect Vitest test (e.g., test exists but doesn't actually grep the bundle). The Vitest contract test in task #9 is what makes the R-14 assertion airtight by deterministically running `vite build --mode production` before grepping. The shell guard's role after #9 is a stale-`dist` tripwire only.

**Why hardcoded path**: the canonical test path `packages/sdk-core/tests/bundle-content.test.ts` is the one source of truth for the transition. Named here in §Q1, in the guard's header comment, and in `docs/TODO.md` §R-14 Transition. If #9's implementer picks a different filename, the guard's expected path is a one-line edit in the same devloop. Trade-off accepted: explicit coupling is the cost of the forcing function.

**Lifecycle after #9 lands** (per §Tech Debt / Deferrals #1): #9's devloop chooses (a) delete this guard, OR (b) keep state 4 only as a stale-dist tripwire.

**6. `exports-map-closed.sh`** (security domain; closed-world exports; rule shape co-signed by @paired-security)
- **Trigger**: any modified `packages/*/package.json`.
- **Allowlist** (exemption from all checks): packages where `"private": true` OR `name` starts with `@darktower/test-`. No separate allowlist file, no extra `darktower:test-only` package.json field per @paired-security (two existing signals cover every legitimate case).
- For each non-exempt `packages/*/package.json`:
  - **Check A — wildcard/test-only subpath KEYS** (HARD VIOLATION per @paired-security S5; substring match, not full-key match): forbid `exports` keys matching regex `(^|/)(test|tests|testing|test-only|__tests__|internal|private)(/|$)`. This catches `./test-only/*`, `./testing/sub`, `./foo/__tests__/x`, `./internal/*`, `./private/*` etc. Also forbid wildcard-only keys like `./*` (re-exports everything).
  - **Check B — test/internal source paths in exports VALUES** (HARD VIOLATION per @paired-security S5; walk values too): for every `exports` subpath, resolve the value (string or `import`/`require`/`types` conditional sub-field) and forbid values pointing into `./src/test/`, `./src/tests/`, `./src/internal/`, `./src/private/`, `./src/__tests__/`, `./test-only/`, `./test/`, regardless of the key's name. This catches `"./util": "./src/test/helpers.ts"` (public name, test source).
  - **Check C — missing `exports` (SOFT-DEFERRED per @paired-security S6)**: a missing `exports` key emits `WARN: package <name> missing 'exports' (closed-world surface not enforceable)` and does NOT increment the violation counter. **Strict mode**: when env var `STRICT_EXPORTS_MAP=1` is set, the warning is promoted to a HARD VIOLATION. Today's behavior: warn-only (some infra-only packages like `proto-gen` may legitimately ship without `exports` while internal). Transition path captured in §Tech Debt / Deferrals #7: flip `STRICT_EXPORTS_MAP=1` in CI once all non-private packages have `exports` (target: end of Wave 2).
- **Rule shape defense** (per @paired-security): ADR-0028 §5 supply chain — public exports are the package's auditable surface; wildcard subpaths or test-only re-exports break audit. Closed-world enumeration is the only auditable shape. Test-only packages are `private: true` so they cannot be installed by external consumers — the allowlist carve-out is safe.

### Discovery change in `run-guards.sh` (reconciled by @operations + @code-reviewer; ruling confirmed by @team-lead)

NUL-delimited iteration with `mapfile -d '' -t` (shellcheck-SC2044-clean), `-not -path '*/fixtures/*'` prune as load-bearing structural invariant, and inline comment anchoring `docs/TODO.md:341`:
```bash
# Iterate simple/**/*.sh recursively. Prune fixtures/ — no committed
# fixtures per docs/TODO.md:341 (2026-05-08 guard-self-test-cleanup);
# pruning structurally enforces the policy at the runner level.
mapfile -d '' -t guards < <(
    find "$SIMPLE_GUARDS_DIR" -name "*.sh" -type f -not -path '*/fixtures/*' -print0 | sort -z
)
for guard in "${guards[@]}"; do
    if [[ -x "$guard" ]]; then
        GUARD_NAME=$(basename "$guard" .sh)
        ...
    fi
done
```

Constraints from @operations sign-off (all preserved):
1. **Executability check kept**: `if [[ -x "$guard" ]]` is the existing contract — non-executable `.sh` files (sourced helpers etc.) must not be auto-run.
2. **NUL-delimited iteration** via `find -print0 | sort -z | mapfile -d '' -t`. Whitespace-safe, shellcheck-SC2044-clean.
3. **`-not -path '*/fixtures/*'` prune is load-bearing**: structural invariant that encodes the 2026-05-08 no-fixtures policy at the runner level. Inline comment anchors `docs/TODO.md:341`. Reconciled with @code-reviewer; @team-lead confirmed the prune stays.
4. **No other behavior change**: `set -euo pipefail`, verbose/non-verbose branching, `VIOLATION|ERROR` grep, `((TOTAL_GUARDS++)) || true`, FAILED_GUARD_NAMES array — all preserved verbatim. Minimal infrastructure edit, not a rewrite.
5. **Existing `simple/*.sh`** continue to match at depth 1; `simple/ts/*.sh` discovered at depth 2.

**Comment update inside `run-guards.sh`**: any inline comment referencing "iterates `simple/*.sh`" updated to "iterates `simple/**/*.sh` recursively (deterministic via `find | sort -z`)".

### Ad-hoc correctness check during implementation

Per the 2026-05-08 policy, no committed fixtures. During implementation I'll build a throwaway scratch directory with positive/negative cases per guard, run each guard against synthetic `GUARD_DIFF_BASE` git history, confirm the pass/fail behavior matches the spec, and discard the scratch dir before commit. This is documented in §Validation when complete.



None.

---

## Implementation Summary

Six TS guards landed under `scripts/guards/simple/ts/`, plus the reconciled
`run-guards.sh` recursion change, ADR-0033 line-99 wording fix, and `docs/TODO.md`
§R-14 Transition entry. All shell scripts pass `bash -n` syntax check.

Per-guard implementation notes:

1. **`no-secrets-in-ts.sh`** (security domain, Mechanical port) — 5 checks: secret-var
   assignments (token DROPPED from identifier list per S1(a)), API key prefixes (sk-/
   pk-/AKIA/ghp_/gho_/xox), connection strings (S2), Authorization headers (S3), JWT
   shape. Long-base64 deferred per S4. Env-lookup exclusions: `process.env.`,
   `import.meta.env.`, `Deno.env.get(`, `Bun.env.`, `globalThis.__VITE_DEFINE__`.
   Path filter inline (no `filter_test_code_ts` pull-up per @dry-reviewer).

2. **`no-pii-in-logs-ts.sh`** (observability domain, Mechanical port) — unified
   log-sink regex `\b(console|logger|log)\.(log|info|warn|error|debug|trace|emit)\(`;
   PII pattern IDENTICAL to Rust (closed-list, additive-only invariant, owner: obs);
   Check 1+2 BLOCKING, Check 3 WARNING (mirrors Rust Check 4). Allowlist includes
   `// pii-safe: <reason>` opt-out, `_hash`/`Hash` suffixes, `[REDACTED]`/`masked`/
   `hashed` markers, comments.

3. **`no-test-removal-ts.sh`** (test domain, file-deletion-only v1 per @test rule-
   owner ruling) — empty-diff idempotence at top; `get_deleted_files` over
   `.test.ts`/`.spec.ts`/`.test.tsx`/`.spec.tsx`/`.test.svelte` and `__tests__/`-
   located `.ts/.tsx`; relaxed match (same basename OR same package). Block-count
   heuristic deferred to §Tech Debt #2 follow-up guard.

4. **`name-guard-dt-client.sh`** (R-26/R-24, ADR-0019 Pattern B) — regex
   `^dt_client_[a-z][a-z0-9_]{0,53}$` per @observability MF-OBS-1. Scope:
   `packages/sdk-core/src/**` + `packages/web-app/src/**`. test-utils exempt with
   `InMemoryMetricsSink.ts:1-8` rationale. Scans literal first-arg of all seven
   Meter factory methods (`createCounter|createHistogram|createUpDownCounter|
   createGauge|createObservable{Counter,Gauge,UpDownCounter}`).

5. **`no-dev-trust-path-in-prod-bundle.sh`** (R-14 forcing function) — four states:
   (1) no `sdk-core/` → exit 0; (2) `sdk-core/` exists BUT
   `packages/sdk-core/tests/bundle-content.test.ts` missing → FAIL with canonical-
   path message (the forcing function); (3) both exist → exit 0 ("Layer 4 carries
   the real check"); (4) belt-and-suspenders dist grep for `serverCertificateHashes`
   if state 3 + dist exists.

6. **`exports-map-closed.sh`** (security closed-world rule) — `jq`-driven walk of
   `packages/*/package.json`. Check A: forbidden KEYS regex
   `(^|/)(test|tests|testing|test-only|__tests__|internal|private)(/|$)` +
   wildcard `./*` (HARD). Check B: walks VALUES (incl. import/require/types
   conditional sub-fields), forbids `./src/test/`/`./src/tests/`/`./src/internal/`/
   `./src/private/`/`./src/__tests__/`/`./test-only/`/`./test/` (HARD). Check C:
   missing `exports` is WARN by default; `STRICT_EXPORTS_MAP=1` promotes to HARD.
   Allowlist: `private: true` OR name prefix `@darktower/test-`.

**Infrastructure changes**:
- `scripts/guards/run-guards.sh`: recursion via `mapfile -d '' -t guards < <(find ... -name "*.sh" -type f -not -path '*/fixtures/*' -print0 | sort -z)`. Executability check, `set -euo pipefail`, VIOLATION/ERROR grep sentinel — all preserved verbatim. Inline comment anchors `docs/TODO.md:341`.
- `docs/decisions/adr-0033-polyglot-validation-pipeline.md` line 99 wording: `runs all simple/*.sh` → `runs all simple/**/*.sh recursively, fixtures pruned`.
- `docs/TODO.md`: added §R-14 Transition entry naming the canonical test path `packages/sdk-core/tests/bundle-content.test.ts`. Co-references §Tech Debt #1 in this devloop output.

---

## Validation

### Layer 3 (Guards) — full pipeline

```
./scripts/guards/run-guards.sh
Total guards run: 29
Passed: 29
Failed: 0
Elapsed time: 7.22 seconds
```

Baseline (pre-change): 23 guards. Post-change: 29 (23 + 6 TS). Wall-clock 7.22s
on empty-TS-diff tree, well within @operations' 9s budget. All six new guards
exit 0 cleanly with informative STATUS lines on the current `HEAD`
(`packages/sdk-core/` and `packages/web-app/` absent).

### Per-guard syntax check

`bash -n` on all six TS guards + `run-guards.sh`: all clean.

### Ad-hoc correctness scripts (run during implementation, NOT committed)

Per the 2026-05-08 no-fixtures policy (`docs/TODO.md:341`), correctness was
demonstrated via throwaway scratch repos with synthetic `GUARD_DIFF_BASE` and
file trees. Scratch dirs were `rm -rf`'d after verification. Results:

**`no-secrets-in-ts.sh`** (4/4 scenarios correct):
| Scenario | Expected | Observed |
|---|---|---|
| Hardcoded `sk-...` prefix | EXIT 1 (Check 2) | EXIT 1 ✓ |
| `process.env.X ?? ""` | EXIT 0 (env exempt) | EXIT 0 ✓ |
| `const token = await fetch(...)` (S1(a) check) | EXIT 0 (token dropped) | EXIT 0 ✓ |
| `"Authorization: Bearer ghp_..."` literal | EXIT 1 (Check 4) | EXIT 1 ✓ |

**`no-pii-in-logs-ts.sh`** (4/4 scenarios correct):
| Scenario | Expected | Observed |
|---|---|---|
| `console.log({ email: u.email })` | EXIT 1 | EXIT 1 ✓ |
| `logger.info({ event_type, meeting_id_hash, trace_id })` | EXIT 0 (R-26 bounded) | EXIT 0 ✓ |
| `console.log({ user_id_hash, email_hash })` | EXIT 0 (_hash) | EXIT 0 ✓ |
| `console.log({ email }); // pii-safe: <reason>` | EXIT 0 (opt-out) | EXIT 0 ✓ |

**`no-test-removal-ts.sh`** (2/2 scenarios correct):
| Scenario | Expected | Observed |
|---|---|---|
| Delete `b.spec.ts`, no replacement | EXIT 1 | EXIT 1 ✓ |
| Delete `b.spec.ts`, add `b2.spec.ts` in same package | EXIT 0 (relaxed match) | EXIT 0 ✓ |

**`name-guard-dt-client.sh`** (5/5 scenarios correct):
| Scenario | Expected | Observed |
|---|---|---|
| No `packages/sdk-core/src/` (state 1) | EXIT 0 (target absent) | EXIT 0 ✓ |
| `createCounter("foo_total")` (no prefix) | EXIT 1 | EXIT 1 ✓ |
| `createCounter("dt_client_FOO_total")` (uppercase) | EXIT 1 | EXIT 1 ✓ |
| `createCounter("dt_client_join_attempts_total")` | EXIT 0 | EXIT 0 ✓ |
| Same bad name in `packages/test-utils/` | EXIT 0 (exempt) | EXIT 0 ✓ |

**`no-dev-trust-path-in-prod-bundle.sh`** (5/5 state-transitions correct):
| State | Expected | Observed |
|---|---|---|
| State 1: no `sdk-core/` | EXIT 0 + STATUS message | EXIT 0 ✓ |
| State 2: `sdk-core/` exists, no canonical test (FORCING) | EXIT 1 + canonical-path message | EXIT 1 ✓ |
| State 3: both exist | EXIT 0 + "Layer 4 carries the real check" | EXIT 0 ✓ |
| State 4: dist contains `serverCertificateHashes` | EXIT 1 + file:line | EXIT 1 ✓ |
| State 4: dist clean | EXIT 0 | EXIT 0 ✓ |

**`exports-map-closed.sh`** (6/6 scenarios correct):
| Scenario | Expected | Observed |
|---|---|---|
| `@darktower/pubpkg` (public) with `./test-only/*` key (Check A) | EXIT 1 | EXIT 1 ✓ |
| Same pkg with `private: true` | EXIT 0 (exempt) | EXIT 0 ✓ |
| `@darktower/test-utilities` name | EXIT 0 (test- prefix exempt) | EXIT 0 ✓ |
| Public key `./util` resolving to `./src/test/helpers.ts` (Check B) | EXIT 1 | EXIT 1 ✓ |
| Public pkg with no `exports` (default) | EXIT 0 + WARN | EXIT 0 ✓ |
| Same with `STRICT_EXPORTS_MAP=1` | EXIT 1 | EXIT 1 ✓ |

### Reviewer-facing verification checklist (per @operations)

- ✓ Baseline 23 → 29 post-change guard count.
- ✓ Wall-clock 7.22s ≤ 9s budget on empty-TS-diff tree.
- ✓ Each new guard exits 0 cleanly on current `HEAD` (target packages absent).
- ✓ Synthetic-probe scenarios produce useful pass/fail output.

### Constraints adhered to

- ✓ `run-guards.sh` executability check preserved (`if [[ -x "$guard" ]]`).
- ✓ NUL-delimited iteration via `find -print0 | sort -z | mapfile -d '' -t`.
- ✓ `-not -path '*/fixtures/*'` prune kept as load-bearing structural invariant; inline comment cites `docs/TODO.md:341`.
- ✓ `set -euo pipefail`, verbose/non-verbose branching, `VIOLATION|ERROR` grep, `((TOTAL_GUARDS++)) || true`, FAILED_GUARD_NAMES — all preserved verbatim.
- ✓ No `filter_test_code_ts` pre-extraction; three filters implemented independently per @dry-reviewer ADR-0019 framework.
- ✓ Cross-Boundary Classification table final (no conditional rows).
- ✓ ADR-0033 line 99 wording updated.
- ✓ `docs/TODO.md` §R-14 Transition entry references canonical test path.

### Gate 2 — full `scripts/layer-all.sh` (Lead, after `pnpm install --frozen-lockfile`)

| Layer | Result | Notes |
|---|---|---|
| 1 (Compile) | Rust OK, TS OK; Proto FAIL | Proto stage-1 `command -v buf` check fails — pre-existing tooling mismatch (task #35 wrappers expect system `buf`; `pnpm exec buf` is the available path). Not a regression of this devloop. |
| 2 (Format) | Rust OK, TS OK (incl. `nx run proto-gen:format` via `pnpm exec buf format`); Proto stage-1 FAIL | Same buf-binary-missing as Layer 1. |
| 3 (Guards) | **OK — 29/29 PASSED in 7.35s** | Direct verification of this devloop's scope. |
| 4 (Test) | Rust OK, TS OK | |
| 5 (Lint) | Rust OK, TS lint partially FAIL (`proto-gen:lint`) | `pnpm exec buf lint` fails on internal.proto/signaling.proto naming + layout violations. **Pre-existing** — these are exactly the violations task #29 will mask via `proto/buf.yaml` `lint.ignore` scaffolding (per user-story Track-2 #29 description). |
| 6 (Audit) | `pnpm audit` OK; `cargo audit` FAIL (RUSTSEC-2023-0071 rsa 0.9.10 Marvin Attack); buf-breaking missing | Both pre-existing. `rsa` advisory is documented in `docs/TODO.md` §"Resolve 6 pre-existing `cargo audit` findings" and accepted at Task #2's main.md Layer 6 verification (no NEW vulnerabilities introduced by this devloop). |
| 7 (Env-tests) | N/A — wave2-pending | Layer 7 wiring lands in task #38. Not blocking. |
| Cross-boundary scope-drift (`validate-cross-boundary-scope.sh`) | OK | Passes now that the guard files are present in the diff. |
| Cross-boundary classification-sanity (`validate-cross-boundary-classification.sh`) | OK | Ran at Gate 1; unchanged. |
| `shellcheck` (artifact-specific) | Not run — binary unavailable in this env | Operations surfaced a TODO.md entry to wire shellcheck into Layer 3 as a follow-up devloop. `bash -n` syntax-only check passes on all changed shell. |
| Semantic-guard (Layer 7) | Pending | Spawned; verdict will land via SendMessage. |

**Regression analysis**: every Gate 2 FAIL is pre-existing — proto wrappers' system-`buf` lookup (introduced in #35), proto-package naming violations (target of #29), `rsa` advisory (accepted at #2). Re-running the pipeline at Start Commit `91cf097` would produce the same failure set. No layer that this devloop's diff touches (Layer 3 guards, `run-guards.sh` discovery shape) regresses.

**Lead decision**: proceed to Gate 3 review on the strength of Layer 3 = 29/29 PASSED + clean cross-boundary scope + classification + the pre-existing-failure documentation above. Reviewer panel will be told the pre-existing failures are not theirs to evaluate as regressions.

---

## Review

### Gate 3 findings landed

**F1 (paired-security, MUST-FIX) — RESOLVED**: `exports-map-closed.sh` Check B replaced `FORBIDDEN_VALUE_PATTERNS` literal-prefix array with a segment regex `FORBIDDEN_VALUE_REGEX='/(test|tests|testing|test-only|__tests__|internal|private)(/|$)'`. The literal array only covered source-tree paths (`./src/test/`, `./src/internal/`, etc.) and left built-artifact paths like `./dist/test-only/helpers.mjs` and `./lib/__tests__/foo.mjs` as a public-named-key + test-source-target bypass. Segment regex mirrors Check A's word-boundary semantics. Ad-hoc verified:
- `./dist/test-only/helpers.mjs` → EXIT 1 ✓ (was bypass)
- `./lib/__tests__/helpers.mjs` → EXIT 1 ✓ (was bypass)
- `./dist/index.mjs` → EXIT 0 ✓ (no regression on normal dist exports)
- `./src/test/helpers.ts` → EXIT 1 ✓ (original case still caught)

**F2 (paired-security, NICE-TO-HAVE — applied)**: `no-dev-trust-path-in-prod-bundle.sh` now resolves `SDK_CORE_DIR`, `CANONICAL_TEST`, `SDK_CORE_DIST` as `$SEARCH_PATH/packages/sdk-core/...` rather than bare relative paths. Prevents silent state-1 fire when invoked outside the repo root. State-2 violation message still names the canonical repo-relative path (`packages/sdk-core/tests/bundle-content.test.ts`) for clarity. Ad-hoc verified state 1 + state 2 from cwd=/tmp with SEARCH_PATH=scratch-dir.

**F3 (paired-security, OPTIONAL/won't-fix-ok)**: accepted as-is per @team-lead. JWT regex lacks closing-quote anchor; matches embedded-in-text JWTs but not exploitable.

**F4 (paired-security, small ask — applied)**: `no-dev-trust-path-in-prod-bundle.sh` state-2 message now bakes the assertion contract into the failure output, not just the canonical test path. The new message states explicitly: *"The canonical contract test MUST grep the prod-mode `vite build` output for the literal string 'serverCertificateHashes' and fail on any hit."* Plus a sentence about `__DEV_TRUST_FINGERPRINT__=false` tree-shaking. Rationale: #9's implementer cannot pick a different filename without ALSO updating this failure message — the assertion contract is now baked into the guard, preventing quiet semantic drift. One-line edit beats unilateral filename change.

**F-TEST-1 (test, MUST-FIX — RESOLVED)**: `no-test-removal-ts.sh` had a basename-as-regex over-match bug. Two-part fix:
1. **Collector**: `collect_test_files()` no longer relies on `common.sh`'s `get_*_files` regex extension filter (which interpreted `.test.ts$` as a regex, falsely matching `featureXtest.ts`). Now uses explicit bash `case` patterns for `*.test.ts`, `*.spec.ts`, `*.test.tsx`, `*.spec.tsx`, `*.test.svelte`, and `*/__tests__/*.{ts,tsx}`. Fix is local to this guard; fixing `common.sh` itself is out of scope (would affect every existing guard).
2. **Matcher**: pre-compute `ADDED_BASENAMES` once outside the per-deletion loop. Replace `grep -q "/${local_base}$"` (regex) with `grep -Fxq -- "$local_base"` (fixed-string, whole-line, basename-only). Replace same-package fallback `grep -q "^${local_pkg}/"` with `grep -Fq -- "${local_pkg}/"`. Complexity drops from O(N×M) to O(N+M).

Ad-hoc scenarios:
- Deleted `feature.test.ts` + unrelated `featureXtest.ts` only → **EXIT 1** ✓ (bypass closed; @test's exact repro)
- Rename `feature.test.ts` → `__tests__/feature.test.ts` → EXIT 0 ✓
- Deleted `feature.test.ts` + different-basename `newfeature.test.ts` in same package → EXIT 0 ✓
- Deleted `feature.test.ts` + only `helper.ts` (non-test) in same package → **EXIT 1** ✓ (collector correctly excludes non-test additions)

**S1 Vite-define exclusion (tightening landed during this round)**: `no-secrets-in-ts.sh` Check 1 exclusion alternation now includes `\b__[A-Z_]+__\b` so Vite-define-substituted literals like `__DEV_TRUST_FINGERPRINT__` and `__BUILD_VERSION__` on RHS are exempt. Approved by @team-lead as in-scope mechanical tightening.

---

## Verdicts

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security (paired) | RESOLVED | F1 (must-fix), F2 (nice-to-have), F4 (small ask) | F1, F2, F4 | F3 (won't-fix-ok), long-base64 → §Tech Debt #6 | F1: built-artifact path bypass in `exports-map-closed.sh` Check B closed via segment regex. F2: SEARCH_PATH-relative paths in `no-dev-trust-path-in-prod-bundle.sh`. F4: state-2 assertion contract baked into failure message. F3: JWT-regex closing-quote anchor accepted as-is. |
| Test | RESOLVED | F-TEST-1 (must-fix, basename-as-regex over-match) | F-TEST-1 (collector rewrite + matcher swap) | block-count guard → §Tech Debt #2; `.only/.skip` guard → §Tech Debt #3 | F-TEST-1 patched via collector rewrite (bash `case` globs) + matcher swap to `grep -Fxq`/`grep -Fq`. Surfaced deeper `common.sh::get_*_files` regex-vs-literal bug, captured in `docs/TODO.md` Polyglot Pipeline Follow-ups. |
| Observability | RESOLVED | F-OBS-1 (medium), F-OBS-2 (low), F-OBS-3 (low) | F-OBS-1, F-OBS-2 | F-OBS-3 (accepted as-is); span.setAttribute → §Tech Debt #9; sdk-svelte scope → §Tech Debt #5 | F-OBS-1: regex tightened to `^dt_client_[a-z]([a-z0-9_]{0,52}[a-z0-9])?$` (no trailing underscore). F-OBS-2: span.setAttribute descoped from plan §2, deferred. F-OBS-3: cross-language parity with Rust accepted. |
| Code Quality | APPROVE | F1–F5 (minor/informational) | F4 (`walk_exports` delimiter comment), F5 (`local_pkg` sed comment) | F1 (`local_*` naming, cosmetic), F2 (FP surface, observability domain), F3 (camelCase regex) → §Tech Debt #10 | TS test-path filter near-identity locked in (`TEST_PATH_EXCLUDES` comment block; future-extraction-mechanical alignment). |
| DRY | PASS | (none blocking; one TODO entry requested) | TODO entry added to `docs/TODO.md` §Cross-Service Duplication (DRY) verbatim | n/a | Per @dry-reviewer's ADR-0019 framework: 3 callers with related-but-distinct shape; defer extraction until 4th true-rhyming caller (e.g. `no-eval-in-ts.sh`). |
| Operations | APPROVE | (none) | n/a | n/a | All five sign-off constraints honored (executability check, NUL-delimited iteration, no other behavior change, fixtures prune, ADR-0033 line-99 wording update). 29/29 pipeline PASSED at 7.5s. |
| Semantic Guard | SAFE | (none) | n/a | n/a | No semantic-guard-flagged concerns. |

---

## Tech Debt / Deferrals

1. **R-14 Transition — `no-dev-trust-path-in-prod-bundle.sh` → Vitest contract test** (forcing-function timeline; canonical entry mirrored to `docs/TODO.md` §R-14 Transition by this devloop). When task #9 lands `packages/sdk-core/`, the implementer of #9 MUST land `packages/sdk-core/tests/bundle-content.test.ts` asserting the prod `vite build --mode production` artifact does not contain `serverCertificateHashes`. **The current `no-dev-trust-path-in-prod-bundle.sh` guard mechanically enforces this**: sdk-core landing without the canonical test will fail CI at state 2. No Nx wiring needed — `scripts/lang/ts/test.sh:10` already covers `nx affected -t test:unit test:component`; precedent `packages/test-utils/project.json`. Once the test lands, #9's devloop chooses (a) delete this guard entirely (Vitest carries the real check) OR (b) keep state 4 only (belt-and-suspenders stale-dist tripwire).

2. **`no-silent-test-deletion-ts.sh` (follow-up guard)** — net `(it|test)(` block-count regression heuristic. Deferred per @test rule-owner judgment (ADR-0024 §6.2 Mechanical-with-owner): block-counting across `it.each(`, `test.skip(`, multi-line invocations, template literals is brittle. Strict v1 (Check 1 file-deletion only) plus defer-heuristic mirrors `docs/TODO.md:341` project pattern. Scope after an incident makes the cost-benefit clear; the file-deletion check (Check 1 in v1) carries the load-bearing signal in the meantime.

3. **`no-only-skip-ts.sh` (follow-up guard)** — detect `.only` / `.skip` in committed test files. Deferred per @test pre-plan until Svelte client lands and a real corpus exists for tuning.

4. **`no-dev-trust-path-in-prod-bundle.sh` state-2 forcing behavior**: today (state 1) the guard exits 0. The moment `packages/sdk-core/` lands without `packages/sdk-core/tests/bundle-content.test.ts`, the guard transitions to state 2 and fails CI with the message in §Per-guard design #5. #9's implementer reads the message, lands the test, and the guard moves to state 3. Lifecycle thereafter managed in #1 above.

5. **`name-guard-dt-client.sh` scope extension to `packages/sdk-svelte/`**: @observability's listed scope is `sdk-core/src/**` + `web-app/src/**` only. If/when `sdk-svelte` introduces a `Meter` instance (not anticipated today), extend the guard scope. Coordinate with @observability at that point.

6. **`no-secrets-in-ts.sh` long-base64 heuristic check (Rust Check 5) — DEFERRED** per @paired-security: TS false-positive rate is much higher than Rust (inlined source-maps, base64-encoded sprites, crypto-key fixtures). Re-evaluate when there's evidence of a missed-credential leak that a base64 heuristic would have caught.

7. **`exports-map-closed.sh` STRICT_EXPORTS_MAP transition**: today missing `exports` is a WARN. Flip `STRICT_EXPORTS_MAP=1` in CI once all non-private packages have `exports`. Target: end of ADR-0033 Wave 2. At that point `proto-gen` and any other infra-only public package must either go `private: true` or define an `exports` field; the warn becomes a hard violation.

8. **Cross-package fixture sharing for `no-secrets-in-ts` (real-JWT cases)** — open coordination, scoped to **task #17**. Per @paired-security: if/when a shared real-JWT fixture is needed across `no-secrets-in-ts.sh` regression evidence and `@darktower/test-utils`'s `./test-only/` exports, task #17 is the right place to revisit. Until then ad-hoc proof during this devloop + Layer-3-runtime verification against synthetic input is sufficient. No carve-out from `docs/TODO.md:341` requested.

9. **OTel tracing-attribute PII scan — `span.setAttribute(...)` / `span.setAttributes({...})`** — DEFERRED per @observability F-OBS-2. The unified log-sink regex in `no-pii-in-logs-ts.sh` does not match tracing-attribute sinks (only logger/console emit). Lands as a future Layer-4 semantic check (TS-AST-aware) once `sdk-core` starts emitting span attributes. Today's R-26 client scope is structured-log-only; tracing attributes are not yet exercised in the join flow. Owner: observability.

10. **camelCase identifiers in `no-secrets-in-ts.sh` Check 1 — DEFERRED** per @code-reviewer F3. The Check 1 regex is snake_case-only (`password|secret|api_key|...`); TS code commonly uses camelCase variants (`apiKey`, `clientSecret`, `privateKey`, `masterKey`). The Rust analog has the same gap by design (Rust convention is snake_case). Re-evaluate when there's an incident or a story that materially shifts TS naming conventions in the repo. Owner: security (rule shape) + client (implementation).

---

## Files Modified

**Created**:
- `scripts/guards/simple/ts/no-secrets-in-ts.sh`
- `scripts/guards/simple/ts/no-pii-in-logs-ts.sh`
- `scripts/guards/simple/ts/no-test-removal-ts.sh`
- `scripts/guards/simple/ts/name-guard-dt-client.sh`
- `scripts/guards/simple/ts/no-dev-trust-path-in-prod-bundle.sh`
- `scripts/guards/simple/ts/exports-map-closed.sh`

**Modified**:
- `scripts/guards/run-guards.sh` — switch to `find -print0 + sort -z + mapfile -d '' -t` recursive iteration, prune `*/fixtures/*`, inline comment anchoring `docs/TODO.md:341`. Loop body unchanged.
- `docs/decisions/adr-0033-polyglot-validation-pipeline.md` — line 99 text update (`simple/*.sh` → `simple/**/*.sh recursively, fixtures pruned`).
- `docs/TODO.md` — added §R-14 Transition section naming canonical test path `packages/sdk-core/tests/bundle-content.test.ts`.
- `docs/devloop-outputs/2026-05-12-ts-guards-task37/main.md` — this file.
