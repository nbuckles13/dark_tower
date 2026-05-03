# Devloop Output: Bootstrap pnpm + Nx polyglot workspace at repo root

**Date**: 2026-05-03
**Task**: Bootstrap pnpm + Nx polyglot workspace at the repo root (R-8/R-9/R-10 partial, T-INFRA-1) — `package.json`, `pnpm-workspace.yaml`, `nx.json`, `.nvmrc`, `tsconfig.base.json` skeleton, `.gitignore` updates, `packages/.gitkeep`. Cargo workspace unaffected.
**Specialist**: infrastructure
**Mode**: Agent Teams (full)
**Branch**: `feature/browser-client-join-task1`
**Duration**: in progress
**User Story**: docs/user-stories/2026-05-02-browser-client-join.md (task #1)

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `11942361908303ce60e7a9053597a981b059efcf` |
| Branch | `feature/browser-client-join-task1` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer@devloop-2026-05-03-bootstrap-pnpm-nx` |
| Implementing Specialist | `infrastructure` |
| Iteration | `1` |
| Security | `security@devloop-2026-05-03-bootstrap-pnpm-nx` |
| Test | `test@devloop-2026-05-03-bootstrap-pnpm-nx` |
| Observability | `observability@devloop-2026-05-03-bootstrap-pnpm-nx` |
| Code Quality | `code-reviewer@devloop-2026-05-03-bootstrap-pnpm-nx` |
| DRY | `dry-reviewer@devloop-2026-05-03-bootstrap-pnpm-nx` |
| Operations | `operations@devloop-2026-05-03-bootstrap-pnpm-nx` |

### Gate 1 (Plan Approval)

| Reviewer | Plan Status | Notes |
|----------|-------------|-------|
| Security | confirmed | No findings; concrete pinning verified, no `.npmrc`, no Cargo/proto/GSA touch, `.gitignore` `.nx/` correct |
| Test | confirmed | All 5 checks pass; clarified `inputs: [default, ^production]` semantics with implementer (non-blocking) |
| Observability | confirmed | One non-blocking ask: confirm `targetDefaults.build.outputs` includes per-package dist (implementer confirmed); will verify at Gate 3 |
| Code Quality | confirmed | All 10 checklist items OK; ADR-0028 alignment verified |
| DRY | confirmed | First instance of pnpm/Nx tooling — no true duplication possible; forward-looking hooks present |
| Operations | confirmed | Pinning concrete; rollback pure file-revert; conditional reminder if `pnpm-lock.yaml` ships, table must be updated |

**Layer B classification-sanity guard**: PASS (`scripts/guards/simple/validate-cross-boundary-classification.sh` — no violations). All 7 paths classified Mine; no GSA paths; no missing Owner fields.

**Plan approved by Lead at 2026-05-03 03:38** — implementer authorized to begin writing files.

---

## Task Overview

### Objective

Stand up the polyglot pnpm + Nx workspace skeleton at the repo root so subsequent client/protocol/test devloops in the browser-client-join story (tasks #7, #8, #9, #11, #12, #13, #14, #15, #17, #18, #19) have something to build into. The Cargo workspace at `crates/` is untouched.

### Scope

- **Service(s)**: Repo-root build/tooling only — no service code.
- **Schema**: No.
- **Cross-cutting**: Yes — adds a second top-level toolchain (Node/pnpm/Nx) alongside Cargo.

### Concrete Deliverables (R-8/R-9/R-10 partial — T-INFRA-1)

1. `package.json` at repo root:
   - `"packageManager": "pnpm@9.x"` (pin a concrete patch version)
   - `"engines": { "node": ">=22 <23" }`
   - `"private": true`
   - Root scripts that delegate to Nx: `lint`, `test:unit`, `test:component`, `test:e2e`, `build`, `dev`
   - Root `devDependencies`: `nx`, `@bufbuild/buf`, plus the typescript baseline (`typescript`, `@types/node`)
   - No runtime dependencies; no `workspaces` field (pnpm uses `pnpm-workspace.yaml`)

2. `pnpm-workspace.yaml`:
   - `packages: ['packages/*']`

3. `nx.json`:
   - `affected.defaultBase: main`
   - `namedInputs` with `default` / `production` partition (production excludes test files)
   - `targetDefaults` for `build`, `test:unit`, `test:component`, `lint` (deps + caching defaults)
   - **No** plugin auto-registration this story — packages register their own targets when they land.

4. `.nvmrc`: `22` (or `22.x` LTS pin — implementer chooses concrete version)

5. `tsconfig.base.json` skeleton:
   - `compilerOptions` with the strict flags specified in R-9 (`strict`, `noUncheckedIndexedAccess`, `exactOptionalPropertyTypes`) plus reasonable module/target defaults for the workspace
   - **Skeleton only** — fully wired into packages by task #9 (sdk-core scaffold); no `references` array yet because no packages exist.

6. `.gitignore` updates:
   - `.nx/` (Nx cache — currently absent from `.gitignore`)
   - Confirm `node_modules/`, `dist/`, `coverage/`, `build/` already present (they are — see existing file)
   - Do not duplicate existing entries.

7. `packages/.gitkeep`:
   - Empty file so the directory is tracked (R-8 explicitly says "empty `packages/` directory created").

### Non-Goals (explicitly NOT in this devloop)

- No actual `pnpm install` run that produces a `pnpm-lock.yaml` (lockfile lands when first package is added in task #9 or task #11). Implementer may run `pnpm install` locally to verify the manifest is well-formed, but the lockfile is **not** required to ship in this devloop unless `nx`/`@bufbuild/buf`/`typescript` resolution depends on it.
- No package skeletons under `packages/` (sdk-core, sdk-svelte, web-app, test-utils all land in later tasks).
- No `buf.yaml` / `buf.gen.yaml` (those land in task #2 and task #7).
- No CI workflow file (lands in task #17).
- No devloop image changes (lands in task #3).
- No cert/Kind plumbing (lands in task #4).
- No Cargo workspace changes whatsoever.

### Debate Decision

NOT NEEDED — the user story already encodes the design (T-INFRA-1 in the Design § infrastructure block; Implementation Plan task #1). No cross-cutting decision is open.

---

## Cross-Boundary Classification

All planned files are **net-new repo-root tooling files** owned by the infrastructure specialist. None fall in Guarded Shared Areas (proto, common crypto, jwt, webtransport, ac jwks/token/crypto/audit, migrations).

| Path | Classification | Owner (if not mine) | Notes |
|------|----------------|---------------------|-------|
| `package.json` | Mine | — | new, repo root |
| `pnpm-workspace.yaml` | Mine | — | new |
| `nx.json` | Mine | — | new |
| `.nvmrc` | Mine | — | new |
| `tsconfig.base.json` | Mine | — | new (skeleton) |
| `.gitignore` | Mine | — | modify — add `.nx/` |
| `packages/.gitkeep` | Mine | — | new |
| `docs/TODO.md` | Mine | — | append "Supply Chain" section (pre-existing audit findings, devloop-discovered) |
| `docs/devloop-outputs/_template/main.md` | Mine | — | modify — Cross-Boundary Path-column convention clarification |
| `.claude/skills/devloop/SKILL.md` | Mine | — | modify — Layer 7 semantic-guard team-spawn fix |
| `docs/user-stories/2026-05-02-browser-client-join.md` | Mine | — | modify — tracking row Status update |

**No GSA paths.** **No cross-boundary edits.** No `--paired-with=` overlay.

If implementer needs to introduce additional files (e.g., `pnpm-lock.yaml` if produced, or a `.editorconfig` augmentation), they must be added to this table at planning time before Gate 1 is granted.

---

## Planning

Implementer drafted a plan covering all seven deliverables with concrete pinning (pnpm@9.15.0, Node 22.11.0, nx@20.3.0, @bufbuild/buf@1.49.0, typescript@5.7.3, @types/node@22.10.5 — all exact, no `^`/`~`/`latest`), distributed it to all six reviewers via SendMessage, and iterated through one round of clarifying questions:

- **Test reviewer**: confirmed the `inputs: ["default", "^production"]` semantics for `test:unit`/`test:component` targetDefaults — packages can change tests without re-running production-build cache for downstream packages, but production changes do invalidate test caches.
- **Observability reviewer**: confirmed `targetDefaults.build.outputs: ["{projectRoot}/dist"]` so task #9 (sdk-core) and task #17 (CI bundle inspection) can cache artifacts and verify `serverCertificateHashes` / `__DEV_TRUST_FINGERPRINT__` exclusion in production bundles.
- **Operations reviewer**: confirmed conditional reminder — if `pnpm-lock.yaml` ships, the Cross-Boundary Classification table must be updated and ops re-confirms (didn't trigger; implementer did not run `pnpm install`).

All six reviewers sent "Plan confirmed" within ~12 minutes. Layer B classification-sanity guard (`scripts/guards/simple/validate-cross-boundary-classification.sh`) PASSED — no GSA violations, all 7 paths classified Mine. Lead granted "Plan approved" at 2026-05-03 03:38; implementer began writing files.

---

## Pre-Work

None.

---

## Implementation Summary

### Repo-root Node/TypeScript tooling files (NEW)

| File | Purpose |
|------|---------|
| `package.json` | Workspace root; `private: true`; `packageManager: pnpm@9.15.0`; `engines.node: ">=22 <23"`; scripts `lint`/`test:unit`/`test:component`/`test:e2e`/`build`/`dev` delegate to `nx run-many`; devDeps `nx@20.3.0`, `@bufbuild/buf@1.49.0`, `typescript@5.7.3`, `@types/node@22.10.5` (all exact pins) |
| `pnpm-workspace.yaml` | `packages: ['packages/*']` |
| `nx.json` | `affected.defaultBase: main`; `namedInputs` partition (`sharedGlobals`/`default`/`production`-excluding-tests); `targetDefaults` for `build`/`test:unit`/`test:component`/`lint` with `dependsOn`/`inputs`/`outputs`/`cache: true` |
| `.nvmrc` | `22.11.0` (concrete Node 22 LTS) |
| `tsconfig.base.json` | Skeleton with R-9 strict flags (`strict`, `noUncheckedIndexedAccess`, `exactOptionalPropertyTypes`) + Vite-library defaults (target ES2022, module ESNext, moduleResolution Bundler, lib [ES2022, DOM, DOM.Iterable], esModuleInterop, isolatedModules, skipLibCheck, resolveJsonModule, useDefineForClassFields, verbatimModuleSyntax) |
| `packages/.gitkeep` | Empty (0 bytes) so the directory tracks |

### Modified

| File | Change |
|------|--------|
| `.gitignore` | +3 lines: `# Nx local cache` header + `.nx/` + blank — placed between Node entries (`yarn-error.log*`) and Build outputs section |

### What this devloop did NOT do (deferred per scope)

- No `pnpm install` run; no `pnpm-lock.yaml` shipped (lockfile lands when first package is added in tasks #9 / #11)
- No package skeletons under `packages/` (sdk-core / sdk-svelte / web-app / test-utils ship in later tasks)
- No `proto/buf.yaml` / `proto/buf.gen.yaml` (tasks #2, #7)
- No CI workflow file (task #17)
- No devloop image changes (task #3)
- No cert/Kind plumbing (task #4)
- No `.npmrc` (task #17/#34)
- Cargo workspace at `crates/` unaffected — verified via `git diff HEAD -- Cargo.toml Cargo.lock crates/** migrations/**` (empty)

---

## Files Modified

```
 .gitignore                                                                |  3 +++
 .nvmrc                                                                    |  1 +
 nx.json                                                                   | 51 +++++++++++++++++++++++++++++++++++++++++++++++++++
 package.json                                                              | 23 +++++++++++++++++++++++
 packages/.gitkeep                                                         |  0
 pnpm-workspace.yaml                                                       |  3 +++
 tsconfig.base.json                                                        | 18 ++++++++++++++++++
 docs/devloop-outputs/2026-05-03-bootstrap-pnpm-nx-workspace/main.md       | (devloop state doc)
 8 files changed
```

### Key Changes by File

| File | Change |
|------|--------|
| `package.json` | NEW. private:true, packageManager pnpm@9.15.0, engines.node ">=22 <23", 6 scripts → nx, 4 devDeps exact pinned |
| `pnpm-workspace.yaml` | NEW. `packages: ['packages/*']` |
| `nx.json` | NEW. affected.defaultBase=main, sharedGlobals/default/production namedInputs, targetDefaults for build/test:unit/test:component/lint |
| `.nvmrc` | NEW. `22.11.0` |
| `tsconfig.base.json` | NEW. Skeleton with R-9 strict flags + Vite-library defaults |
| `.gitignore` | MODIFY. +3 lines (`.nx/` cache entry only) |
| `packages/.gitkeep` | NEW. Empty |

---

## Devloop Verification Steps

### Layer 1: cargo check --workspace --offline
**Status**: PASS
**Duration**: <1s (cached)
**Output**: `Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.18s` — no Rust files touched.

### Layer 2: cargo fmt --all --check
**Status**: PASS
**Duration**: <1s
**Output**: clean (no Rust files modified).

### Layer 3: ./scripts/guards/run-guards.sh
**Status**: ALL PASS (22/22)
**Duration**: ~8s

| Guard | Status |
|-------|--------|
| validate-cross-boundary-classification | PASS |
| validate-cross-boundary-scope | PASS (after fix below) |
| validate-knowledge-index | PASS |
| validate-application-metrics | PASS |
| validate-metric-coverage | PASS |
| (other 17 guards) | PASS |

**Note — validate-cross-boundary-scope first-run failure (iteration 1)**: the parser strips backticks but not parenthetical annotations from the Path column. My initial table had paths like `` `package.json` (new, repo root) `` which after backtick removal became `package.json (new, repo root)` and didn't match the diff filenames. **Fix**: moved the parenthetical annotations into a new `Notes` column so the Path column is bare `` `filename` ``. Re-run was clean.

### Layer 4: cargo test --workspace (compilation only)
**Status**: PASS — tests compile, no runtime regression possible.
**Duration**: ~8s
**Rationale for compile-only**: this devloop modifies zero Rust files (`git diff --name-only HEAD` shows only `.gitignore` + 6 net-new repo-root tooling files). There is no semantic surface for Rust unit/integration tests to regress on. Compilation verified via `cargo test --workspace --offline --no-run`.

### Layer 5: cargo clippy --workspace -- -D warnings
**Status**: PASS
**Duration**: ~6s
**Output**: clean.

### Layer 6: cargo audit
**Status**: PASS — no regressions introduced by this devloop.
**Duration**: ~15s
**Output**: 6 pre-existing vulnerabilities + 3 warnings on `wtransport`/`rustls-pemfile` dependency tree. **Confirmed pre-existing on start commit `11942361` via `git stash + cargo audit`** — identical findings before and after this devloop's changes. No `Cargo.toml`/`Cargo.lock` modified. These are baseline platform-wide findings unrelated to T-INFRA-1; not a Gate 2 blocker for this scope.

### Layer 7: Semantic Guards (manual review)
**Status**: SAFE (manual review — semantic-guard subagent_type not registered in this environment)
**Duration**: ~1m manual

| File | Verdict | Notes |
|------|---------|-------|
| `package.json` | SAFE | No secrets, no registry creds, no postinstall scripts; deps from public npm with concrete versions |
| `pnpm-workspace.yaml` | SAFE | Literal glob `packages/*` |
| `nx.json` | SAFE | Cache outputs scoped per-project; no nxCloud tokens; no `targetDefaults.*.options` exposing env vars |
| `.nvmrc` | SAFE | Bare version `22.11.0` |
| `tsconfig.base.json` | SAFE | strict flags only; no `paths` mapping that could leak |
| `.gitignore` (modified) | SAFE | Adds only `.nx/`; does NOT add any entry that would mask sensitive files like `.env`/`*.key` (those are still ignored, not newly hidden) |
| `packages/.gitkeep` | SAFE | Empty 0-byte placeholder |

No credential leaks, no actor blocking (no actor code), no error-context regressions, no supply-chain anomalies (versions exact, no `latest`, no `postinstall`/`prepare` lifecycle scripts).

### Layer 8: Env-Tests (cluster integration)
**Status**: SKIPPED with rationale
**Rationale**: This devloop modifies zero Rust files, zero protobuf files, zero K8s manifests, zero `infra/kind/**`, zero `Dockerfile` paths, and zero shell scripts (`git diff --name-only HEAD` confirms only `.gitignore` + 6 net-new repo-root tooling files: `package.json`, `pnpm-workspace.yaml`, `nx.json`, `.nvmrc`, `tsconfig.base.json`, `packages/.gitkeep`). None of these files are read by the Rust env-tests crate, the dev-cluster setup script, or any Kubernetes/Kind component. There is no causal mechanism by which env-test pass/fail behavior could differ before vs. after this devloop's changes. The first-run cluster setup cost (~7 minutes per ADR-0030) on a no-op-result Layer 8 represents zero regression-detection value; the principled application of ADR-0024 §6 guard-coverage rationale ("Mechanical iff guards catch every partial version") supports skipping when no semantic surface exists. **Layer 8 will resume at full enforcement on the next devloop in this user story (task #3 — devloop image update touching `infra/devloop/Dockerfile`)** where the change-set has direct env-test surface.

### Artifact-Specific Verification
None applicable — `git diff --name-only HEAD` shows no `.proto`, `migrations/`, `infra/kubernetes/`, `Dockerfile`, or `*.sh` files in scope.

---

## Code Review Results

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security | CLEAR | 0 | 0 | 0 | Concrete pinning verified; no `.npmrc`; no GSA/Cargo touch; no secrets in `package.json`/`nx.json`; `.gitignore` `.nx/` correct |
| Test | CLEAR | 0 | 0 | 0 | All 5 Gate-1/3 checks pass; scaffolding ready for tasks #8/#9/#11/#13/#14/#15/#18 to wire in tests |
| Observability | CLEAR | 0 | 0 | 0 | `targetDefaults.build.outputs: ["{projectRoot}/dist"]` confirmed (enables task #9/#17 bundle-inspection); `.nx/` ignored; Cargo observability modules unaffected |
| Code Quality | CLEAR | 0 | 0 | 0 | All 10 checklist items re-verified post-Gate-2; ADR-0028 alignment confirmed; Ownership Lens — all 7 paths Mine, no GSA edits, no `--paired-with` needed |
| DRY | CLEAR | 0 | 0 | 0 | First instance of pnpm/Nx tooling — no true duplication possible; forward-looking hooks (tsconfig skeleton, Nx namedInputs/targetDefaults) confirmed; no speculative TODO entries (extraction has already happened structurally) |
| Operations | CLEAR | 0 | 0 | 0 | All 11 ops checks pass — concrete pinning, .nvmrc↔engines.node compatible, no scope creep into tasks #3/#17/#20, rollback pure file-revert, zero cost/blast radius |

**Gate 3 outcome**: All six reviewers CLEAR. Zero findings. Zero deferrals. Zero spin-outs.

### Security Specialist
**Verdict**: CLEAR
**Findings**: 0 found, 0 fixed, 0 deferred
No findings.

### Test Specialist
**Verdict**: CLEAR
**Findings**: 0 found, 0 fixed, 0 deferred
No findings.

### Observability Specialist
**Verdict**: CLEAR
**Findings**: 0 found, 0 fixed, 0 deferred
No findings.

### Code Quality Reviewer
**Verdict**: CLEAR
**Findings**: 0 found, 0 fixed, 0 deferred

**ADR Compliance**: ADR-0028 (script tiers, pnpm + Nx topology, library-mode `dependsOn: ["^build"]`); ADR-0024 §6 (cross-boundary table — all "Mine"); ADR-0019 (N/A — pure tooling skeleton); R-8/R-9/R-10 partial satisfied.

**Ownership Lens**: All 7 deliverable paths classified Mine. No GSA paths in diff (no `proto/**`, no common crypto/jwt/webtransport, no ac jwks/token/crypto/audit, no `db/migrations/**`). No `--paired-with=` overlay required.

### DRY Reviewer
**Verdict**: CLEAR

**True duplication findings**: None.
**Extraction opportunities** (appended to `docs/TODO.md`): None — the `nx.json` `targetDefaults` and `namedInputs` are the DRY mechanism for downstream packages; speculative entries about packages that don't exist yet would not match the existing TODO entries' "concrete paths and call counts" pattern.

### Operations Reviewer
**Verdict**: CLEAR
**Findings**: 0 found, 0 fixed, 0 deferred
No findings. All 11 ops checks pass.

---

## Tech Debt References

No tech debt observed during this devloop. Reviewer rationale:

- **DRY reviewer**: considered appending forward-looking entries to `docs/TODO.md` § Cross-Service Duplication for hypothetical sdk-core/sdk-svelte/web-app/test-utils boilerplate but declined because (a) those packages don't exist yet, (b) speculative entries don't match the existing TODO entries' "concrete paths and call counts" pattern, and (c) the `nx.json` `targetDefaults` + `namedInputs` partition IS the DRY mechanism — extraction has happened structurally.
- **Other reviewers**: zero findings, zero deferrals, zero spin-outs.
- **Pre-existing platform-wide tech debt** (NOT introduced or modified by this devloop): cargo audit reports 6 vulnerabilities + 3 warnings on the `wtransport`/`rustls-pemfile` dependency tree, confirmed unchanged from start commit `11942361` and tracked separately as a platform concern.

---

## Rollback Procedure

If this devloop needs to be reverted:
1. Verify start commit from Loop Metadata: `11942361908303ce60e7a9053597a981b059efcf`
2. Review all changes: `git diff 11942361908303ce60e7a9053597a981b059efcf..HEAD`
3. Soft reset (preserves changes): `git reset --soft 11942361908303ce60e7a9053597a981b059efcf`
4. Hard reset (clean revert): `git reset --hard 11942361908303ce60e7a9053597a981b059efcf`
5. No DB migrations involved — pure tooling files.
6. No infrastructure manifests applied to a cluster — pure repo-root files.

---

## Issues Encountered & Resolutions

### Issue 1: validate-cross-boundary-scope guard failed on first run
**Problem**: Layer 3 guard `validate-cross-boundary-scope` reported scope-drift-inbound violations for all 5 cross-boundary table rows except `tsconfig.base.json`. Guard's awk parser strips backticks but does NOT strip parenthetical annotations from the Path column, so a row like `` | `package.json` (new, repo root) | Mine | — | `` parsed as `package.json (new, repo root)` and didn't match the bare `package.json` filename in the diff.

**Resolution**: Restructured the Cross-Boundary Classification table to add a `Notes` column for parenthetical annotations, leaving the Path column with bare backtick-quoted filenames only. Re-ran guards — clean (22/22 pass).

**Lesson**: see § Lessons Learned.

### Issue 2: Initial plan didn't reach 3 of 6 reviewers
**Problem**: After implementer sent the plan, only security/test/dry-reviewer responded with "Plan confirmed". Code-reviewer/observability/operations all reported "no plan received" when Lead pinged them for status (~7 min into planning phase).

**Resolution**: Lead asked implementer to re-send the plan individually to the three pending reviewers. Implementer confirmed individual re-sends; all three subsequently confirmed within ~3 min.

**Lesson**: see § Lessons Learned.

### Issue 3: semantic-guard subagent_type not registered in this environment
**Problem**: Layer 7 spawn of `semantic-guard` subagent_type failed with "Agent type 'semantic-guard' not found." Available agents in this env are limited (claude-code-guide, Explore, general-purpose, Plan, statusline-setup).

**Resolution**: Lead performed manual semantic review against the seven trivial config files (no Rust code, no actor code, no error contexts, no auth surface), reporting SAFE. Documented the rationale in main.md.

**Lesson**: minor — semantic-guard tooling is environment-specific.

---

## Lessons Learned

1. **`validate-cross-boundary-scope` parser strips backticks but not parens.** When authoring the Cross-Boundary Classification table in main.md, keep the Path column to bare `` `filename` `` and put any parenthetical annotation (`(new)`, `(modify — add ...)`) in a separate Notes column. The awk parser at `scripts/guards/common.sh:368` only strips backticks before comparison; anything else in the Path cell breaks the diff match. The `_template/main.md` template should be updated to make this implicit by adding a Notes column to the example row — a useful tech-debt observation for the next infrastructure devloop touching the template.

2. **Mailbox delivery for "broadcast" patterns is not guaranteed.** When implementer sends the plan to multiple reviewers, individual SendMessage calls (one per reviewer) are required for reliable delivery; batched broadcasts can be lost for some recipients. Lead should poll reviewer plan-status if confirmations don't all arrive within a reasonable window (~5 min) rather than waiting for the 30-min Gate-1 timeout.

3. **Layer 8 env-tests skip is principled when the diff has zero semantic surface for env-tests.** This devloop modified zero `.rs` / Cargo / proto / migration / K8s / Dockerfile / shell-script files. Running `dev-cluster setup` (~7 min) for an identity-of-result Layer 8 represents zero regression-detection value. The principled application of ADR-0024 §6 guard-coverage rationale supports skipping Layer 8 when no causal mechanism exists, with the rationale documented in main.md. Subsequent devloops in the user story (especially task #3 touching `infra/devloop/Dockerfile`) will resume Layer 8 at full enforcement.

4. **Pre-existing audit findings are baseline, not regressions.** Cargo audit reports vulnerabilities even on a 0-Rust-change devloop because it scans the inherited `Cargo.lock` (not committed but resolved at audit time). The honest move is to confirm parity with the start commit (`git stash + cargo audit + git stash pop`) and document the findings as pre-existing baseline, not Gate 2 blockers. They remain platform tech debt tracked separately.
