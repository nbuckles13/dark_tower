# Devloop Output: ci-client.yml CI Workflow (task #17)

**Date**: 2026-06-23
**Task**: New `.github/workflows/ci-client.yml` (R-47, R-7 CI gates, R-33, R-34) — path-triggered lint+unit+component jobs; eslint+prettier+svelte-check toolchain; buf lint/breaking/format; client TS guards; pnpm audit; `@vitest/coverage-v8` ≥90% gate; component browser-mode Chromium; Dependabot config.
**Specialist**: infrastructure
**Mode**: Agent Teams (v2) — full
**Branch**: `feature/browser-client-join-task-17`
**Duration**: ~55m (setup → complete; 2 Gate-2 attempts, 0 review iterations)

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `7bc9aa4547308a621fa689fe55bc64ff3049b65e` |
| Branch | `feature/browser-client-join-task-17` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer@session` |
| Implementing Specialist | `infrastructure` |
| Iteration | `2` |
| Security | `CLEAR` |
| Test | `CLEAR` |
| Observability | `CLEAR` |
| Code Quality | `CLEAR` |
| DRY | `CLEAR` |
| Operations | `CLEAR` |
| Semantic Guard | `CLEAR` |
| Client (conditional — cross-boundary owner of `packages/**`) | `CLEAR` |

### Gate 3 — Reviewer Verdicts

| Reviewer | Verdict | Findings | Notes |
|----------|---------|----------|-------|
| Security | **CLEAR** | 0 | permissions block, S1 audit fail-closed, R-33 guards, no committed secrets, minimatch pre-existing — all verified |
| Test | **CLEAR** | 0 | T1 coverage gate real+green (95.83% branch from dead `__DEV_TRUST_FINGERPRINT__` false-arm, expected); T2 base-ref diff confirmed; test-utils no gate |
| Observability | **CLEAR** | 0 | no instrumentation surface; Codecov `flags: client-unit` isolates client coverage; job/step naming diagnosable |
| Code Quality | **CLEAR** | 0 | ADR-0034/0028 compliant; full Ownership Lens — all 10 prettier files Mechanical-confirmed, 5 config rows Minor-judgment; no GSA; YAML/ESLint/Prettier config quality high |
| DRY | **CLEAR** | 0 | reuse confirmed (audit.sh, ts guards); 2 extraction opportunities tracked in `docs/TODO.md` (not true-dup → CLEAR) |
| Operations | **CLEAR** | 0 | O1 fixed (line 226); concurrency/permissions/caching/dependabot all confirmed |
| Semantic Guard | **CLEAR** | 0 | S1 deny-by-default audit + S2 dt-guard fail-fast verified clean; no secret echo |
| Client | **CLEAR** | 0 | owner-confirm 6 config hunks + C1 green + 10 prettier reformats Mechanical; cosmetic comment inaccuracy surfaced (Lead pre-commit cleanup, done) |

**GATE 3 OUTCOME: APPROVED — 8/8 CLEAR, zero findings, zero deferrals, zero escalations.**

Pre-commit Lead cleanup (done): corrected the cosmetic `packages/sdk-core/vitest.config.ts` coverage comment (Client-noted; not a reviewer finding) — it wrongly stated `src/index.ts` is coverage-excluded; it is included and the gate is green. Re-verified: unit 18/18 + coverage green, `prettier --check` clean, scope guard clean.

### Gate 2 — Validation

**Attempt 1: FAIL (Layer 3 — scope-drift).** Layers 1,2 PASS; 32/33 guards pass; `validate-cross-boundary-scope` (Layer A) FAILS. Causes: (1) garbled Path cells — parser `canonicalize_path_cell` strips only backticks + one trailing `(annotation)`, so rows with prose/two-paths/backticked-non-path-tokens don't resolve; (2) missing rows — Prettier reformatted 10 client `.ts`/test files + `pnpm-lock.yaml`, absent from table (`diff ∖ plan` ≠ ∅). Routed back (iteration 2): rebuild table, one clean-path row per diff file; prettier-reformatted client files = Not mine/Mechanical (deterministic, guard-covered, non-GSA → review-only), `pnpm-lock.yaml` = Mine/Mechanical.

**Attempt 2: PASS (exit 0).** L1 Compile OK · L2 Format OK · L3 Guards OK (33/33, incl. both cross-boundary guards) · L4 Test OK (Rust + TS unit/component; sdk-core coverage stmts 100%/branches 95.83%/funcs 100%/lines 100%, ≥90% gate active) · L5 Lint OK (eslint+prettier+svelte-check+buf) · L6 Audit **cargo-audit + pnpm-audit both PASS** (aggregate N/A only from proto-audit intentional gap) · L7 Env-tests N/A (`wave2-pending` — env-test layer is a repo-wide not-yet-wired placeholder, not a failure of this change). N/A layers are documented intentional-gap/placeholder cases (not FAIL-MISSING-VERB) → no implementer action owed per ADR-0033.

### Gate 1 — Plan Confirmation

| Reviewer | Plan Status |
|----------|-------------|
| Security | confirmed (Gate-2 note: least-privilege `permissions:` block — done in plan) |
| Test | confirmed (conditions T1, T2 below — folded into plan) |
| Observability | confirmed (out of domain) |
| Code Quality | confirmed (ADR-0034/0028 compliant; no classification upgrades) |
| DRY | confirmed (2 extraction opportunities → `docs/TODO.md`; no blockers) |
| Operations | confirmed (cache-key fix O1; topology suggestion non-blocking) |
| Semantic Guard | confirmed (2 Gate-3 verification notes S1, S2) |
| Client | confirmed (owner-accept all 6 Minor-judgment hunks; coverage caveat C1) |

**ALL 8 GATE-1 REVIEWERS CONFIRMED.**

**Conditions to satisfy during implementation (verified at Gate 2/3):**
- **T1 (critical)**: `sdk-core` `test:unit` is `vitest run` with NO `--coverage` flag — the `thresholds` block would be inert (silent pass at 0%). Add `--coverage` to the run OR set `coverage.enabled: true` in `vitest.config.ts`.
- **T2**: confirm `no-test-removal` guard diffs against a base ref in CI (`fetch-depth: 0` + base-ref mechanism), not a full-tree scan.
- **C1 (client)**: RUN `vitest run --coverage` before committing the hard ≥90% gate; if any metric <90% (likely barrel `src/index.ts` re-exports), add a barrel-importing test OR exclude `src/index.ts` from coverage. Do NOT ship a day-one-red gate.
- **O1 (operations)**: Playwright/Chromium cache key → `hashFiles('pnpm-lock.yaml')` (authoritative version pin), not `package.json`.
- **S1/S2 (semantic-guard, Gate-3 verify)**: audit step must exit non-zero on unsuppressed HIGH/CRITICAL (deny-by-default allowlist, not exit-code invert); lint job must fail-fast if `dt-guard` binary is missing (not a silent no-op pass).
- **Operations topology suggestion (NON-BINDING)**: optionally isolate `dt-guard-build` into its own job. Lead call: single lint job is fine for first-ship (rust-cache keeps warm builds ~30s); implementer's discretion.

**Lead rulings on implementer open questions:**
1. **push trigger** — mirror `ci.yml`: trigger on PR + push to `main`/`develop`.
2. **dt-guard Rust build in client lint job** — accepted (ADR-0034 made guards a Rust binary; R-33 mandates them in `ci-client.yml`). Use `Swatinem/rust-cache`. Operations + code-reviewer to confirm runtime cost at Gate 1.
3. **minimatch override** — verify at impl with `pnpm audit`; if the existing override already clears the 3 HIGH advisories, do NOT add a duplicate, and note the `vitest-coverage-v8-tooling`/minimatch TODO resolution.
4. **coverage scope** — gate `sdk-core` at ≥90% (R-41). Do NOT impose a 90% gate on `test-utils` (test doubles/infra). Test reviewer to confirm.

---

## Task Overview

### Objective
Land the client CI workflow `.github/workflows/ci-client.yml` that gates the pnpm/Nx polyglot client workspace, mirroring the backend `ci.yml` shape (lint + unit + per-tier tests). Three path-triggered jobs:

- **lint**: `pnpm install --frozen-lockfile` (R-34); `pnpm lint` (eslint + prettier + svelte-check); `buf lint` + `buf breaking` (against `main`) + `buf format --diff --exit-code`; client TS guards `no-secrets-in-code` + `no-test-removal` (R-33); `pnpm audit --audit-level=high` (R-34, high/critical fail).
- **unit**: `pnpm test:unit` (Vitest `--run`, Node) with `@vitest/coverage-v8` ≥90% gate.
- **component**: `pnpm test:component` (Vitest browser-mode Chromium, no live cluster).

Plus `.github/dependabot.yml` extended for the GH-Actions ecosystem (npm already present; cargo already present).

### Scope
- **Service(s)**: CI/CD only — `.github/workflows/`, `.github/dependabot.yml`, root lint toolchain, `packages/*` lint targets.
- **Schema**: No
- **Cross-cutting**: Yes — root lint toolchain rollout touches client-owned `packages/**` configs.

### Debate Decision
NOT NEEDED — implementation of an already-decided requirement set (R-47/R-7/R-33/R-34). Toolchain choices (ESLint flat config, Prettier, svelte-check) are established by R-9 and were explicitly routed to infrastructure as deferred work from task #9.

---

## Context Notes (Lead, at setup)

Established before spawning teammates — for implementer + reviewers:

1. **Dependencies satisfied**: task #1 (monorepo bootstrap — `package.json`, `nx.json`, `pnpm-workspace.yaml`, `.nvmrc` Node 22 present), #9 (sdk-core scaffold), #31 (proto STANDARD rename → `proto/dark_tower/{internal,signaling}/v1/`, `proto/buf.yaml` clean) — all Completed. R-61 prereq for the buf gate is satisfied (#31 cleaned the 21 STANDARD findings).
2. **Deferred from task #9, landing here**:
   - R-9 ESLint/Prettier/svelte-check root-lint toolchain was *explicitly routed to infrastructure* — today every package's `lint` target is `tsc --noEmit`; no eslint/prettier config exists at root.
   - `@vitest/coverage-v8` tooling for the ≥90% gate (task #9 deferred it; task #17 description owns the gate).
3. **Already exists** (do not duplicate): `.github/dependabot.yml` (cargo + npm `directory: "/"`) — **needs a `github-actions` ecosystem entry added**, not a rewrite. TS guards `scripts/guards/simple/ts/{no-secrets-in-ts,no-test-removal-ts}.sh` already implement R-33 (CI invokes them; don't re-author). `scripts/lang/ts/audit.sh` already encodes the suppression-aware pnpm-audit logic. `.codecov.yml` exists. `@bufbuild/buf` is a root devDep (use `pnpm exec buf`).
4. **Sequencing reality**: tasks #15 (sdk-svelte / web-app, `.svelte` files), #18/#19 (Playwright E2E) are NOT yet done. So:
   - **No `.svelte` files exist yet** — svelte-check must be wired but no-op-safe (activate when sdk-svelte/web-app land).
   - sdk-core's current `test:component` runs in **node** env (the `bundle-content` production-build test), NOT browser mode. The browser-mode Chromium component tests (R-43) arrive with task #15 — so the `component` job must **provision Chromium now** (forward-compat) even though today's component tier is node-based.
   - **No browser E2E in CI** this story (R-47/R-48) — E2E is the client analogue of `crates/env-tests/`, runs against live Kind, not GitHub Actions.
5. **Workflow shape**: `ci-client.yml` is a **standalone** client workflow (NOT a `scripts/layer-all.sh` call) — explicit job/step commands per R-47. Backend `ci.yml` is unchanged and coexists.
6. **Cross-boundary**: `client` added as conditional 8th reviewer — the lint-toolchain rollout edits client-owned `packages/*/package.json` / `project.json` lint targets.

---

## Cross-Boundary Classification

Per ADR-0024 §6.2. "Mine" = infrastructure domain. "Not mine" = client domain (owner: `client` specialist).
One row per file in the diff. Path cell contains path only (optionally backtick-quoted with one trailing annotation).
`docs/TODO.md` and `main.md` itself are auto-exempt by the scope guard.

| Path | Mine / Not mine | Owner (if not mine) | Classification |
|------|-----------------|---------------------|----------------|
| `.github/workflows/ci-client.yml` (new) | Mine | — | New CI workflow; infrastructure owns CI/CD |
| `.github/dependabot.yml` | Mine | — | Additive github-actions ecosystem entry; infrastructure owns |
| `package.json` | Mine | — | Workspace-wide toolchain devDeps + type:module + lint script; same scope as task #1 pnpm/nx additions |
| `eslint.config.js` (new) | Mine | — | Root ESLint v9 flat config; infrastructure owns CI toolchain wiring |
| `.prettierrc.json` (new) | Mine | — | Root Prettier config; infrastructure owns |
| `.prettierignore` (new) | Mine | — | Root Prettier ignore rules; infrastructure owns |
| `pnpm-lock.yaml` | Mine | — | Mechanical: additive lockfile update for new devDeps |
| `packages/sdk-core/package.json` | Not mine | `client` | Minor-judgment: adds eslint+prettier to lint script and --coverage to test:unit; mechanical wiring per R-9/R-41 spec |
| `packages/sdk-core/project.json` | Not mine | `client` | Minor-judgment: mirrors package.json lint + test:unit command changes in Nx target |
| `packages/sdk-core/vitest.config.ts` | Not mine | `client` | Minor-judgment: adds coverage thresholds (≥90%) per R-41; value spec-driven, no design discretion |
| `packages/sdk-core/src/framing/length-prefix.ts` | Not mine | `client` | Mechanical: Prettier formatting only; zero logic change; verified by passing tests |
| `packages/sdk-core/src/index.ts` | Not mine | `client` | Mechanical: Prettier formatting only; zero logic change |
| `packages/sdk-core/src/transport/BrowserWebTransport.ts` | Not mine | `client` | Mechanical: Prettier formatting only; zero logic change |
| `packages/sdk-core/tests/bundle-content.test.ts` | Not mine | `client` | Mechanical: Prettier formatting only; zero logic change; test still passes |
| `packages/test-utils/package.json` | Not mine | `client` | Minor-judgment: adds eslint+prettier to lint script; mechanical wiring per R-9 spec |
| `packages/test-utils/project.json` | Not mine | `client` | Minor-judgment: mirrors package.json lint command change in Nx target |
| `packages/test-utils/src/InMemoryMetricsSink.ts` | Not mine | `client` | Mechanical: Prettier formatting only; zero logic change |
| `packages/test-utils/src/MockOTLPExporter.ts` | Not mine | `client` | Mechanical: Prettier formatting only; zero logic change |
| `packages/test-utils/src/MockWebTransport.ts` | Not mine | `client` | Mechanical: Prettier formatting only; zero logic change |
| `packages/test-utils/src/index.ts` | Not mine | `client` | Mechanical: Prettier formatting only; zero logic change |
| `packages/test-utils/src/test-only/signer.ts` | Not mine | `client` | Mechanical: Prettier formatting only; zero logic change |
| `packages/test-utils/src/__tests__/MockOTLPExporter.test.ts` | Not mine | `client` | Mechanical: Prettier formatting only; zero logic change; tests pass |
| `packages/test-utils/src/__tests__/TestTokenSigner.test.ts` | Not mine | `client` | Mechanical: Prettier formatting only; zero logic change; tests pass |
| `packages/test-utils/src/__tests__/deterministic-ids.test.ts` | Not mine | `client` | Mechanical: Prettier formatting only; zero logic change; tests pass |

---

## Planning

### Investigation Findings

**Current state of lint targets**:
- All packages (`sdk-core`, `test-utils`, `proto-gen`) have `lint` targets in `project.json` that run `tsc --noEmit` only.
- No ESLint config exists anywhere in the repo today. No Prettier config either.
- `package.json` root `scripts.lint` already runs `nx run-many -t lint` — so adding ESLint/Prettier to per-package lint targets will flow through to `pnpm lint` automatically.

**Current state of coverage**:
- `packages/sdk-core/vitest.config.ts` already declares `coverage: { provider: 'v8', reporter: [...] }` — but `@vitest/coverage-v8` is NOT installed (tracked in `docs/TODO.md` slug `vitest-coverage-v8-tooling`).
- The `thresholds` field is absent from the coverage config — adding it enforces the gate at the Vitest level (fails the run if coverage drops below threshold) rather than requiring a separate CI step.
- `packages/test-utils/vitest.config.ts` (if it exists) likely has the same gap — will verify during implementation.

**Current state of component tests**:
- `packages/sdk-core/vitest.component.config.ts` runs in `environment: 'node'` — this is a production `vite build` bundle-content test, NOT a browser-mode test. This tier currently does not need Chromium.
- Browser-mode Chromium component tests (R-43) arrive with task #15. The `component` job in `ci-client.yml` must provision Chromium NOW (forward-compat) even though today's test is node-based, so task #15 can add browser-mode tests without needing a workflow change.
- Provisioning Chromium via `npx playwright install --with-deps chromium` is forward-compatible and safe even when no browser tests actually run today.

**Guard invocation in CI**:
- `scripts/guards/simple/ts/no-secrets-in-ts.sh` and `no-test-removal-ts.sh` both delegate to `crates/dt-guard/src/ts_secrets.rs` / `ts_test_removal.rs` via the `_dt_guard_wrapper.sh` → `dt-guard` binary.
- The binary lives at `target/release/dt-guard` (built by `scripts/lang/rust/compile.sh`).
- `ci-client.yml` is a standalone client workflow (no Rust build). Options:
  1. Build `dt-guard` as a step in the lint job before invoking the guards.
  2. Set `DT_GUARD` env var to a pre-built path (not viable standalone).
  3. Cache the `dt-guard` binary across jobs via `actions/cache`.
  **Decision**: Build `dt-guard --release` as a step in the lint job, caching via `Swatinem/rust-cache` (same pattern as `ci.yml`). The binary is fast to build from cache. This keeps CI self-contained with no external dependencies. The Rust toolchain install is a one-time addition.

**Audit wrapper decision**:
- `scripts/lang/ts/audit.sh` is the suppression-aware wrapper. It reads `.pnpm-audit-ignore.json`, applies post-hoc filtering, and supports the skip-if-no-dep-change gate.
- `ci-client.yml` is NOT a devloop layer-runner, so the `audit_dep_changed_ts` gate logic (which checks git diffs vs. a cache) is not meaningful in CI context — CI always runs fresh.
- **Decision**: Call `scripts/lang/ts/audit.sh` directly rather than plain `pnpm audit`. The wrapper's suppression logic is load-bearing (the 3 minimatch advisories and GHSA-h67p-54hq-rp68 are already suppressed in `.pnpm-audit-ignore.json`/`audit-suppressions.toml`). This means the audit step is automatically fail-closed for unsuppressed advisories and correctly passes for legitimately suppressed ones without requiring CI-specific flag overrides. The `DEVLOOP_AUDIT_FORCE_RUN=1` env var can force the run (bypassing the no-diff skip gate). Set it in CI to always run audit.

**Minimatch advisory handling**:
- `docs/TODO.md` records 3 HIGH minimatch advisories (GHSA-3ppc-4f35-3m26, GHSA-7r86-cg39-jmmj, GHSA-23c5-xmqv-rm74). These are NOT yet in `audit-suppressions.toml` / `.pnpm-audit-ignore.json`.
- Task #17's constraint from TODO.md: either (i) bump/override and resolve, (ii) add to allowlist with rationale, or (iii) run non-blocking. 
- **Decision**: Add a `pnpm overrides` entry to `package.json` pinning `minimatch@>=9.0.7` (Option b from TODO.md). The root `package.json` already has a `pnpm.overrides` block (contains `minimatch@>=9.0.0 <9.0.7: >=9.0.7` and `form-data` entries). This is the remediation path with minimal blast radius. This resolves the 3 HIGH findings cleanly and avoids polluting `audit-suppressions.toml` with dev-tooling findings that have a real fix. **Security note**: the override is pinning within the same major series (9.x) — not a breaking jump.

**Buf gate**:
- `proto/buf.yaml` is v2/STANDARD. Task #31 completed the STANDARD rename — `proto/dark_tower/{internal,signaling}/v1/` files present, buf lint is clean.
- `buf breaking --against '.git#branch=main'` will run against `main` branch (requires `fetch-depth: 0` for git history access).
- `buf format --diff --exit-code` checks formatting without modifying files.
- All run via `pnpm exec buf` (buf is a root devDep at `@bufbuild/buf: 1.49.0`).

**svelte-check no-op-safe wiring**:
- No `.svelte` files exist yet (task #15 brings them). `svelte-check` must be installed and wired into `pnpm lint` but MUST not fail when there are zero `.svelte` files.
- `svelte-check` with `--threshold error` exits 0 when there are no files to check. With `--input-dir packages` and no `.svelte` files present, it exits 0 cleanly.
- **Decision**: Add `svelte-check` as a root devDep. Wire it into `pnpm lint` via a root-level script that runs `svelte-check --threshold error`. Each package's `lint` target does NOT separately invoke svelte-check (root handles it). This avoids per-package changes for the svelte-check step.

**ESLint flat config layout**:
- Modern ESLint (v9+) uses a flat config (`eslint.config.js` at repo root). This is the correct choice for a new setup.
- Config will include: `@eslint/js` recommended rules, `typescript-eslint` for TS support, and later `eslint-plugin-svelte` (when task #15 lands `.svelte` files). Svelte plugin entry can be added to the flat config now (as a no-op when no `.svelte` files exist) or deferred.
- **Decision**: Land ESLint flat config with JS + TS rules now. Add a placeholder comment for svelte plugin so task #15 only needs to uncomment it. This avoids a workflow change at task #15.
- Config targets `packages/**/*.ts` and `packages/**/*.svelte`. Proto-gen has no TS source to lint.

**Prettier config**:
- `.prettierrc.json` at repo root with standard settings (single quotes, trailing commas, 2-space indent, 100 char print width). `.prettierignore` excludes `node_modules/`, `dist/`, `packages/*/src/proto/` (generated proto files), `packages/*/coverage/`.

**Coverage gate mechanism**:
- **Decision**: Enforce via Vitest `thresholds` in `packages/sdk-core/vitest.config.ts` (add `thresholds: { lines: 90, functions: 90, branches: 90, statements: 90 }`). This means `pnpm test:unit` (which runs `vitest run --coverage`) fails if coverage drops below 90%. CI doesn't need a separate check script — Vitest exits non-zero.
- Coverage report is uploaded to Codecov separately (same pattern as Rust `coverage` job in `ci.yml`). Use `codecov/codecov-action@v4` with `flags: client-unit` to distinguish from Rust coverage.
- The `@vitest/coverage-v8` package is added as a workspace root devDep (matches the existing vitest config's `provider: 'v8'` declaration).

**Component job Chromium provisioning**:
- Install Playwright as a root devDep (for the browser binary management). Use `npx playwright install --with-deps chromium` to install Chromium and system deps.
- Cache the Playwright browser binary using `~/.cache/ms-playwright` keyed on `playwright-version` (from `package.json` or `npx playwright --version`).
- Today's `test:component` uses `environment: 'node'` — the Chromium install is a no-op for this test. When task #15 adds browser-mode tests, the binary is already available.

**pnpm/Node setup**:
- Mirror `ci.yml`: `pnpm/action-setup@v4` with `version: 10.33.2`, `actions/setup-node@v4` with `node-version: '22'` and `cache: 'pnpm'`.

**buf-setup-action**: Mirror `ci.yml` — use `bufbuild/buf-setup-action@v1`.

**Concurrency cancel**:
- Add `concurrency: { group: "ci-client-${{ github.ref }}", cancel-in-progress: true }` at workflow level to cancel in-flight runs on new pushes to the same branch.

**fetch-depth**:
- `lint` job needs `fetch-depth: 0` for `buf breaking --against '.git#branch=main'`.
- `unit` and `component` jobs do not need git history — omit `fetch-depth` (defaults to 1).

**`permissions:` block (least-privilege `GITHUB_TOKEN`)**:
- Workflow-level default: `permissions: contents: read` — sufficient for checkout and read-only operations.
- `unit` job needs `id-token: write` if Codecov OIDC is used (preferred — avoids storing `CODECOV_TOKEN`). If using the `CODECOV_TOKEN` secret instead (matching `ci.yml`'s pattern), `id-token: write` is not needed.
- **Decision**: Mirror `ci.yml` and use `CODECOV_TOKEN` secret for Codecov upload (no OIDC). Workflow-level `permissions: contents: read`. No job-level `permissions` overrides needed — all three jobs are read-only. Explicit declaration prevents accidental permission escalation on future step additions.
- Full block: `permissions: contents: read` at workflow level; no per-job overrides.

**R-33 JWT fixture coverage note**:
- The requirement says "real-JWT fixture coverage." The `dt-guard` binary already has unit tests in `crates/dt-guard/src/ts_secrets.rs` with JWT pattern matching (see `jwt_literal_matches_three_segment_shape` and `check2_source_scan_subset_catches_class_v_only`). These are library-level tests. The CI invocation of the guard on the repo's actual TS files (`no-secrets-in-ts.sh` → `dt-guard ts-no-secrets`) constitutes the positive fixture check (any committed JWT literal in `.ts`/`.svelte` files would be caught). There are no committed JWT literals in the codebase (a true-positive would be a security incident), so the "real-JWT fixture coverage" refers to the existing unit tests in `ts_secrets.rs`. No additional fixture files need to be committed to the repo — this is already covered.

---

### Files to Create / Modify

#### New files
1. **`.github/workflows/ci-client.yml`** — the new standalone client CI workflow
2. **`eslint.config.js`** (repo root) — ESLint v9 flat config for packages/**
3. **`.prettierrc.json`** (repo root) — Prettier formatting config
4. **`.prettierignore`** (repo root) — Prettier ignore rules

#### Modified files
5. **`.github/dependabot.yml`** — ADD `github-actions` ecosystem entry (weekly). Do NOT touch existing `cargo` or `npm` entries.
6. **`package.json`** (root) — ADD `@vitest/coverage-v8`, `eslint`, `@eslint/js`, `typescript-eslint`, `prettier`, `svelte-check` as devDeps; extend `pnpm.overrides` with `minimatch>=9.0.7` remediation; optionally add Playwright as a devDep for Chromium provisioning.
7. **`packages/sdk-core/package.json`** — update `lint` script (add eslint + prettier calls); add `@vitest/coverage-v8` note (installed at root, available workspace-wide).
8. **`packages/sdk-core/project.json`** — update `lint` target command.
9. **`packages/sdk-core/vitest.config.ts`** — add `thresholds: { lines: 90, functions: 90, branches: 90, statements: 90 }` to coverage config.
10. **`packages/test-utils/package.json`** — update `lint` script.
11. **`packages/test-utils/project.json`** — update `lint` target.
12. **`packages/proto-gen/project.json`** — add a no-op `lint` target (or leave as-is if `nx run-many --passWithNoTests` equivalent skips missing targets — need to verify with `nx`'s `--nx-bail` behavior).

---

### Workflow Structure: `ci-client.yml`

```
name: CI Client

on:
  push:
    branches: [main, develop]
  pull_request:
    branches: [main, develop]
    paths:
      - 'packages/**'
      - 'proto/**'
      - 'pnpm-lock.yaml'
      - 'nx.json'
      - 'package.json'
      - 'pnpm-workspace.yaml'
      - 'tsconfig.base.json'
      - 'proto/buf.yaml'
      - 'proto/buf.gen.yaml'
      - '.github/workflows/ci-client.yml'

permissions:
  contents: read   # least-privilege; no per-job overrides needed

concurrency:
  group: ci-client-${{ github.ref }}
  cancel-in-progress: true

env:
  CARGO_TERM_COLOR: always   # match ci.yml convention

jobs:
  lint:  (ubuntu-latest, timeout 20m)
    - checkout (fetch-depth: 0 — needed for buf breaking --against main)
    - install system deps (protobuf-compiler for dt-guard build)
    - install Rust stable (for dt-guard build; nightly not needed for dt-guard itself)
    - rust-cache (shared-key: ci-client-dt-guard)
    - build dt-guard: cargo build --release -p dt-guard --quiet
    - install pnpm (pnpm/action-setup@v4, version: 10.33.2)
    - install Node 22 (actions/setup-node@v4, cache: pnpm)
    - buf-setup-action@v1
    - pnpm install --frozen-lockfile
    - pnpm lint  (nx run-many -t lint: tsc + eslint + prettier per package + svelte-check at root)
    - buf lint (cwd: proto, `pnpm exec buf lint`)
    - buf breaking (cwd: proto, `pnpm exec buf breaking --against '.git#branch=main'`)
    - buf format check (cwd: proto, `pnpm exec buf format --diff --exit-code`)
    - no-secrets guard: bash scripts/guards/simple/ts/no-secrets-in-ts.sh
    - no-test-removal guard: bash scripts/guards/simple/ts/no-test-removal-ts.sh
    - audit: DEVLOOP_AUDIT_FORCE_RUN=1 bash scripts/lang/ts/audit.sh

  unit:  (ubuntu-latest, timeout 15m)
    - checkout (default fetch-depth: 1)
    - install pnpm (action-setup@v4, version: 10.33.2)
    - install Node 22 (setup-node@v4, cache: pnpm)
    - pnpm install --frozen-lockfile
    - pnpm test:unit  (nx run-many -t test:unit; vitest runs with --coverage; sdk-core thresholds
                       gate at ≥90%; exits non-zero if below)
    - upload coverage (codecov/codecov-action@v4, flags: client-unit,
                       token: ${{ secrets.CODECOV_TOKEN }})

  component:  (ubuntu-latest, timeout 20m)
    - checkout (default fetch-depth: 1)
    - install pnpm (action-setup@v4, version: 10.33.2)
    - install Node 22 (setup-node@v4, cache: pnpm)
    - pnpm install --frozen-lockfile
    - restore Playwright browser cache (actions/cache, key: playwright-chromium-${{ hashFiles('package.json') }},
                                        path: ~/.cache/ms-playwright)
    - install Playwright Chromium: npx playwright install --with-deps chromium
    - save browser cache (post-restore if cache miss)
    - pnpm test:component  (nx run-many -t test:component; today = node env vite-build bundle test)
```

---

### Lead Rulings (received 2026-06-23)

1. **push trigger**: Mirror `ci.yml` — trigger on PR + push to `main`/`develop`. Path filters apply to `pull_request` only. **Resolved.**

2. **dt-guard Rust build in client lint job**: Accepted. Build `dt-guard --release` in lint job with `Swatinem/rust-cache`, `shared-key: ci-client-dt-guard`. **Resolved.**

3. **Minimatch override**: Verified at implementation — existing `pnpm.overrides` entry already resolves all 3 HIGH advisories. `pnpm audit --audit-level=high` exits 0 (1 moderate only). No new override added. **Resolved.**

4. **Coverage scope**: Gate `sdk-core` at ≥90% only (R-41). `test-utils` excluded from hard gate. **Resolved.**

5. **`permissions:` block**: `contents: read` at workflow level, no per-job overrides. **Resolved.**

### Gate-1 Test Conditions (T1, T2)

**T1 (coverage gate must be real AND green)**: Fixed. `test:unit` now runs `vitest run --coverage`. Added `thresholds: {...90...}` to `vitest.config.ts`. Verified: gate is active and green (see §Implementation Summary for actual numbers). **Resolved.**

**T2 (no-test-removal base ref)**: `dt-guard ts-no-test-removal` → `get_diff_base()` → `scripts/lang/_get_base_ref.sh`. In CI PR mode: `git merge-base origin/$GITHUB_BASE_REF HEAD`. Push mode: `HEAD~1`. Both require `fetch-depth: 0` — already on the lint job for `buf breaking`. **Resolved.**

### Gate-1 Operations / Security Conditions (O1, S1, S2)

**O1 (Playwright cache key)**: `hashFiles('pnpm-lock.yaml')` — lockfile is the authoritative pin. **Resolved.**

**S1 (audit deny-by-default)**: `scripts/lang/ts/audit.sh` is deny-by-default via `.pnpm-audit-ignore.json` allowlist. `DEVLOOP_AUDIT_FORCE_RUN=1` set in CI. Confirmed `STATUS=OK` exit 0 locally. **Resolved.**

**S2 (dt-guard binary missing = hard fail)**: `_dt_guard_wrapper.sh` emits `STATUS=FAIL REASON=dt-guard-binary-missing` and exits 1 when binary absent. No extra check in workflow needed. **Resolved.**

---

## Pre-Work

None.

---

## Implementation Summary

### Coverage gate verification (T1, C1)

Fixed `test:unit` to `vitest run --coverage` in both `packages/sdk-core/package.json` and `packages/sdk-core/project.json`. Added `thresholds: { statements: 90, branches: 90, functions: 90, lines: 90 }` to `packages/sdk-core/vitest.config.ts`. Gate confirmed active and green:

| Metric | Actual | Threshold | Result |
|--------|--------|-----------|--------|
| Statements | 100% (68/68) | ≥90% | PASS |
| Branches | 95.83% (23/24) | ≥90% | PASS |
| Functions | 100% (14/14) | ≥90% | PASS |
| Lines | 100% (67/67) | ≥90% | PASS |

`src/index.ts` barrel **is included** in coverage (`include: ['src/**/*.ts']` matches it); its re-export lines score as covered, which is why lines coverage is 100%. No barrel-importing test or exclusion was needed (Client's C1 day-one-red risk did not materialize). The `vitest.config.ts` comment was corrected pre-commit to state this accurately (was previously, incorrectly, "excluded").

### Base-ref mechanism (T2)

`dt-guard ts-no-test-removal` calls `crates/dt-guard/src/common/git_changes::get_diff_base()` → `scripts/lang/_get_base_ref.sh`:
- **PR mode** (`GITHUB_EVENT_NAME == pull_request`): `git merge-base origin/$GITHUB_BASE_REF HEAD` — resolves to the branch point. Requires full history → `fetch-depth: 0` on lint job (already planned for `buf breaking`).
- **Push mode**: `HEAD~1`. Also needs reachable history → same `fetch-depth: 0`.
- **Fail-closed**: script exits 2 if merge-base cannot be resolved, causing guard to abort.

### Minimatch advisory (resolved by existing override)

Verified: `pnpm audit --audit-level=high` exits 0 with only 1 moderate remaining. The existing `pnpm.overrides` entry `"minimatch@>=9.0.0 <9.0.7": ">=9.0.7"` already resolves all 3 HIGH advisories (GHSA-3ppc-4f35-3m26, GHSA-7r86-cg39-jmmj, GHSA-23c5-xmqv-rm74). No new override needed. The TODO.md entry for these advisories can be marked resolved.

### Guard binary existence (S2)

`_dt_guard_wrapper.sh` emits `STATUS=FAIL REASON=dt-guard-binary-missing` and `exit 1` when the binary is absent or non-executable. No additional check in the workflow is needed — the wrapper is already fail-closed. Confirmed by code review of `scripts/guards/simple/_dt_guard_wrapper.sh`.

### Audit deny-by-default (S1)

`scripts/lang/ts/audit.sh` runs `pnpm audit --json`, filters via `.pnpm-audit-ignore.json` (the only sanctioned suppression channel), and exits non-zero if any unsuppressed high/critical advisory remains. The CI step sets `DEVLOOP_AUDIT_FORCE_RUN=1` to bypass the no-dep-change skip gate so CI always scans. Confirmed passing: `STATUS=OK REASON=pnpm-audit-passed`.

### Prettier formatting applied

12 source files required Prettier formatting changes. Applied with `prettier --write "packages/**/*.ts"`. All files now pass `prettier --check`. Tests confirm no behavior change (18/18 unit tests still pass).

### `"type": "module"` added to root package.json

ESLint flat config uses ESM `import` syntax; Node emits a performance warning without `"type": "module"`. Added to root `package.json`. No other effects: pnpm workspaces handle module resolution per-package via their own `"type"` fields.

### svelte-check no-op confirmed

`svelte-check --workspace packages/ --threshold error` exits 0 with `0 FILES 0 ERRORS` when no `.svelte` files exist. No-op-safe until task #15.

---

## Files Modified

| File | Change |
|------|--------|
| `.github/workflows/ci-client.yml` | Created — 3-job standalone client CI workflow |
| `.github/dependabot.yml` | Added `github-actions` ecosystem entry (weekly) |
| `package.json` (root) | Added `"type": "module"`; added `eslint`, `@eslint/js`, `typescript-eslint`, `prettier`, `svelte-check`, `@vitest/coverage-v8` devDeps; updated `lint` script to include svelte-check |
| `eslint.config.js` (new, root) | ESLint v9 flat config for `packages/**/*.ts` |
| `.prettierrc.json` (new, root) | Prettier config (singleQuote, trailingComma, printWidth 100) |
| `.prettierignore` (new, root) | Excludes node_modules, dist, proto-generated TS, pnpm-lock.yaml |
| `packages/sdk-core/package.json` | `test:unit`: `vitest run` → `vitest run --coverage`; `lint`: added `eslint src/ && prettier --check` |
| `packages/sdk-core/project.json` | Same commands in Nx targets |
| `packages/sdk-core/vitest.config.ts` | Added `thresholds: {statements:90, branches:90, functions:90, lines:90}` |
| `packages/sdk-core/src/` (5 files) | Prettier formatting applied (no logic change) |
| `packages/test-utils/package.json` | `lint`: added `eslint src/ && prettier --check` |
| `packages/test-utils/project.json` | Same in Nx target |
| `packages/test-utils/src/` (7 files) | Prettier formatting applied (no logic change) |
| `pnpm-lock.yaml` | Updated for new devDeps |

---

## Code Review Results

**Gate 3: 8/8 CLEAR — zero findings, zero deferrals, zero escalations.** (Full verdict table in §Loop State → Gate 3.)

- **Security** — CLEAR. `permissions: contents: read` present; audit deny-by-default (S1) verified; R-33 guards wired; no committed secrets/JWTs; minimatch resolved by pre-existing override (no new suppression).
- **Test** — CLEAR. Coverage gate real (T1: `--coverage` + thresholds) and green; T2 base-ref diff confirmed; test-utils correctly ungated.
- **Observability** — CLEAR. No instrumentation surface; Codecov `flags: client-unit` isolates client coverage.
- **Code Quality** — CLEAR. ADR-0034 + ADR-0028 compliant. Full Ownership Lens: all 10 Prettier-reformatted client files confirmed Mechanical (sed-test clean, `prettier --check` covers every partial state, non-GSA); 5 config rows Minor-judgment; no GSA touched.
- **DRY** — CLEAR. Reuse confirmed (`scripts/lang/ts/audit.sh`, `scripts/guards/simple/ts/*.sh`); 2 extraction opportunities tracked in `docs/TODO.md` (not true-duplication → CLEAR, not RESOLVED-DEFERRED, per ADR-0019 DRY exception).
- **Operations** — CLEAR. O1 cache-key fix (`hashFiles('pnpm-lock.yaml')`) verified; concurrency/permissions/caching/dependabot confirmed; single-job topology accepted.
- **Semantic Guard** — CLEAR. S1 (audit fail-closed) + S2 (dt-guard fail-fast on missing binary) verified clean; no secret echo.
- **Client** (cross-boundary owner) — CLEAR. Owner-confirmed all 6 Minor-judgment config hunks + the 10 Mechanical Prettier reformats; C1 resolved (gate green). Surfaced one cosmetic comment inaccuracy (handled as a Lead pre-commit cleanup, below).

**Pre-commit Lead cleanup** (not a reviewer finding): corrected the `packages/sdk-core/vitest.config.ts` coverage comment (it wrongly claimed `src/index.ts` was excluded). Re-verified: unit 18/18 + coverage green, `prettier --check` clean, scope guard clean.

---

## Accepted Deferrals

- (none surfaced in this devloop — zero findings deferred, zero spun-out)

The two DRY entries in `docs/TODO.md` (GH Actions setup-step duplication; Nx `targetDefaults.lint` extraction) are **extraction opportunities** under the ADR-0019 DRY exception, not deferred findings — they never entered the fix-or-defer flow, so they are not Accepted Deferrals. The minimatch TODO entry is **resolved** (pre-existing override clears all 3 HIGH advisories; confirmed by the passing audit gate).

---

## Rollback Procedure

If this devloop needs to be reverted:
1. Start commit: `7bc9aa4547308a621fa689fe55bc64ff3049b65e`
2. Review all changes: `git diff 7bc9aa4..HEAD`
3. Soft reset (preserves changes): `git reset --soft 7bc9aa4`
4. Hard reset (clean revert): `git reset --hard 7bc9aa4`
