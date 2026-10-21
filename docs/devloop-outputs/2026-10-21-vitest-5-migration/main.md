# Devloop Output: Migrate TS client monorepo from vitest 4 to vitest 5

**Date**: 2026-10-21
**Task**: Move every vitest package (vitest, @vitest/browser, @vitest/browser-playwright, @vitest/coverage-v8, vitest-browser-svelte) to vitest 5.0.x together; replaces Dependabot PR #78.
**Specialist**: client
**Mode**: Agent Teams (v2) — full, Gate-1 present
**Branch**: `feature/vitest-5`
**Duration**: ~16h wall clock to Gate 3 (setup 02:15 → Gate 3 18:15 UTC, 2026-10-21, including an idle gap waiting on the operator); Gate 2 run after the host image rebuild (`--continue`), ~25 min

---

## Loop Metadata

| Field | Value |
|-------|-------|
| Start Commit | `2d412583969d023a3d2ead1ae09021471a79a0f1` |
| Branch | `feature/vitest-5` |
| Lead Model | `claude-opus-5-5[1m]` |

---

## Loop State (Internal)

| Field | Value |
|-------|-------|
| Phase | `complete` |
| Implementer | `implementer` |
| Implementing Specialist | `client` |
| Tier | `full` |
| Iteration | `1` |
| Security | `security` |
| Test | `test` |
| Observability | `observability` |
| Code Quality | `code-reviewer` |
| DRY | `dry-reviewer` |
| Operations | `operations` |
| Semantic Guard | `not spawned — diff is dependency manifests, lockfile and test config; no checks.md surface (credential types, Debug/Display over payloads, client credential lifetime)` |

---

## Task Overview

### Objective
Replace Dependabot PR #78 (partial bump: vitest + vitest-browser-svelte only, `@vitest/*` left on 4.1.10 → version skew) with a coordinated bump of all vitest packages to the latest 5.0.x, passing Layers 1, 4, 5, 6 and client CI.

### Scope
- **Service(s)**: TS client monorepo (`packages/*`, root `package.json`, `pnpm-lock.yaml`)
- **Schema**: No
- **Cross-cutting**: No (client-only)

### Lead setup findings (facts the task text did not match)
- **Target version**: latest 5.0.x is **5.0.5** for all four `vitest`/`@vitest/*` packages (published 2026-10-15). The task's "(all published 2026-09-03)" matches 5.0.0, not the latest; the operative instruction "latest 5.0.x" is followed. 5.0.5 is older than pnpm's default 1-day `minimumReleaseAge`.
- **Root `package.json` has no `vitest` dependency** — only `@vitest/coverage-v8` (`^4.1.10`). Not adding one (would be a new dependency, not a bump).
- **`vitest-browser-svelte` is already `^3.1.0`** in sdk-svelte and web-app (not 3.0.0); 3.1.0 peers `vitest: ^4.0.0 || ^5.0.0`, and 3.1.0 is the latest published.

### Debate Decision
NOT NEEDED - dependency migration within one specialist's domain.

### Process note
Teammate prompts point each teammate at its `docs/specialist-knowledge/{name}/INDEX.md` and `.claude/skills/devloop/review-protocol.md` to read verbatim as its first action, instead of the Lead pasting ~120 KB of the same text into seven prompts.

---

## Cross-Boundary Classification

| Path | Classification | Owner (if not mine) |
|------|----------------|---------------------|
| `package.json` | Mine | — |
| `packages/sdk-core/package.json` | Mine | — |
| `packages/sdk-svelte/package.json` | Mine | — |
| `packages/test-utils/package.json` | Mine | — |
| `packages/web-app/package.json` | Mine | — |
| `pnpm-lock.yaml` | Mine | — |
| `pnpm-workspace.yaml` | Mine | — |
| `packages/sdk-core/src/telemetry/telemetryConfig.ts` | Mine | — |
| `packages/sdk-core/src/telemetry/__tests__/otlpExporterMock.ts` | Mine | — |
| `packages/sdk-core/src/telemetry/__tests__/otlpExporterSetup.ts` | Mine | — |
| `packages/sdk-core/src/telemetry/__tests__/telemetryConfig.delta.test.ts` | Mine | — |
| `packages/sdk-core/vitest.config.ts` | Mine | — |
| `packages/sdk-core/src/telemetry/__tests__/telemetryConfig.test.ts` | Mine | — |
| `packages/sdk-core/src/signaling/__tests__/signaling-client.test.ts` | Mine | — |
| `packages/sdk-core/src/session/__tests__/meeting-session.test.ts` | Mine | — |
| `packages/sdk-core/src/media/__tests__/media-transport.test.ts` | Mine | — |
| `packages/web-app/src/__tests__/inMeeting.test.ts` | Mine | — |
| `packages/web-app/src/__tests__/joinMeeting.test.ts` | Mine | — |
| `packages/web-app/src/__tests__/authViews.test.ts` | Mine | — |
| `packages/sdk-svelte/vitest.config.ts` | Mine | — |
| `packages/web-app/vitest.config.ts` | Mine | — |
| `packages/sdk-svelte/src/__tests__/MeetingStore.test.ts` | Mine | — |
| `packages/sdk-svelte/README.md` | Mine | — |
| `packages/web-app/README.md` | Mine | — |
| `docs/runbooks/client-dev-local.md` | Mine | — |
| `scripts/lang/ts/compile.sh` | Not mine, Minor-judgment | infrastructure |
| `scripts/lang/_pnpm.sh` | Not mine, Minor-judgment | infrastructure |
| `scripts/lang/ts/fmt.sh` | Not mine, Minor-judgment | infrastructure |
| `scripts/lang/ts/lint.sh` | Not mine, Minor-judgment | infrastructure |
| `scripts/lang/ts/test.sh` | Not mine, Minor-judgment | infrastructure |
| `docs/runbooks/devloop-validation.md` | Not mine, Mechanical | infrastructure |
| `.gitignore` | Not mine, Mechanical | infrastructure |
| `docs/TODO.md` | Not mine, Mechanical | test |
| `docs/devloop-outputs/2026-10-21-vitest-5-migration/main.md` | Mine | — |

The reason for each row is in §Implementation Summary.

---

## Planning

### Baseline (start commit `2d412583`, measured before any change)

Measured per package with `pnpm run test:{unit,component}` after `nx run proto-gen:codegen`, cold Vite cache.
Browser tests ran with `PLAYWRIGHT_BROWSERS_PATH=<scratch>` holding `chromium_headless_shell-1248`
(see Environment blocker below) — the env var was on the measuring command only, never in a tracked file.

| Package | Target | Files | Tests | Coverage emitted |
|---------|--------|-------|-------|------------------|
| sdk-core | test:unit | 70 | 1028 passed | yes — `coverage/` (text/json/html), 78 files, All files 96.63 / 90.99 / 96.07 / 97.76 |
| sdk-core | test:component (node env) | 1 | 4 passed | no (no `--coverage`) |
| test-utils | test:unit | 7 | 19 passed | no (coverage block configured, script has no `--coverage`) |
| sdk-svelte | test:component (browser) | 4 | 27 passed | no |
| web-app | test:unit (node) | 11 | 224 passed | no |
| web-app | test:component (browser) | 13 | 113 passed | no |

Total 1415 tests, 0 skipped. The first web-app component run on this tree showed 4 failures in
`joinMeeting.test.ts` (strict-mode locator violations: render containers not cleaned up); the log shows
`optimized dependencies changed. reloading` / `Vite unexpectedly reloaded a test` for
`@bufbuild/protobuf/codegenv2` — a stale pre-existing `node_modules/.vite` optimizer cache from before the
2026-10-20 bump. Three subsequent cold-cache runs (`.vite` removed) were 113/113 and never reloaded.
(A first attempt without nx codegen failed on missing `signaling_pb.js` — measuring error, not a baseline.)

### Environment blocker (host-side)
`/opt/ms-playwright` in the devloop image has `chromium_headless_shell-1234` (Playwright 1.62.1); 8fbb6075 moved
Playwright to 1.64.1, which needs `-1248`. Browser tests fail at launch on the unchanged start commit.
Remaining action: rebuild the devloop image. Reported to Lead.

### Spike on a scratch worktree (full bump to 5.0.5)
- `pnpm install` resolves vitest, @vitest/browser, @vitest/browser-playwright, @vitest/coverage-v8 at 5.0.5;
  `vitest-browser-svelte@3.1.0(...)(vitest@5.0.5)`; no 4.x left; no `minimumReleaseAgeExclude` written.
  New transitive packages from upstream: `@vitest/ui@5.0.5` (a direct dependency of `@vitest/browser` 5),
  `@vitest/istanbul-lib-{coverage,report}@1.0.2` (coverage-v8 5).
- #78 symptoms under the full bump: `createBrowserServer ... 'project'` — gone (skew).
  `MediaStore.test.ts` `expect.element` "not callable" — gone; sdk-svelte typecheck + lint pass (skew).
  Coverage "Expected string coverage payload" — gone; sdk-core coverage identical (same 78 files, same %).
  OTel "duplicate registration": **PRE-EXISTING on 4.1.10, identical on 5.0.5. Not caused by vitest 5.**
  See §OTel duplicate registration below. (My first "0 everywhere" count was vacuous and is withdrawn.)
- **Browser-tier OTel resolution failure under the full bump (verified).** `@vitest/browser` 5 (`dist/index.js`, optimizeDeps setup) now always
  `exclude.push("@opentelemetry/api")` unless `experimental.openTelemetry` is on (v4 never excluded it).
  So the pre-bundled OTel SDK chunk (`@opentelemetry/core` etc., reached via the sdk-core source alias) keeps
  a bare `import ... from "@opentelemetry/api"`, which Vite resolves from the deps chunk under
  `packages/{sdk-svelte,web-app}/node_modules/.vite/`. pnpm's isolated layout does not link
  `@opentelemetry/api` there — only sdk-core declares it — so every browser test file fails to load
  ("Failed to resolve import @opentelemetry/api"). `optimizeDeps.include` cannot override it (vitest's
  exclude wins; verified). Declaring `@opentelemetry/api` in sdk-svelte and web-app fixes it (verified 27/27).
  These two packages compile sdk-core's *source* into their test bundles, so they genuinely consume its
  dependency graph. *Inferred, not measured:* if the declared version drifted from sdk-core's, pnpm would
  install two copies (the source graph resolving one, the pre-bundled chunk the other), and both would compete
  for the shared `Symbol.for('opentelemetry.js.api.1')` global. That is the reason to give the version one source.
  The additions are **devDependencies only** (sdk-svelte, web-app). The production web-app bundle resolves OTel
  through the sdk-core source alias, as before. `tests/bundle-content.test.ts` passed in the 5.0.5 run
  (web-app node tier 224/224).
- `toHaveTextContent` now checks strict equality (guide §"`toHaveTextContent` Now Performs Strict Equality"):
  7 assertions fail plus 1 typecheck/lint error (RegExp arg) in web-app. One assertion that is NOT failing is
  the important one: `joinMeeting.test.ts:67` `.not.toHaveTextContent('SECRET_TOKEN_123')` — a credential-leak
  guard that under strict equality passes vacuously, because the text is never *exactly* the secret.
- pnpm audit: before 6 moderate / 0 high / 0 critical; after 4 moderate / 0 high / 0 critical.
- Lockfile (5.0.5 tree), one version each, zero 4.x: `vitest@5.0.5`, `@vitest/{browser,browser-playwright,
  coverage-v8,mocker,pretty-format,spy,utils,ui}@5.0.5`. `@vitest/{expect,runner,snapshot}` are gone from
  the graph (guide §"Package Migration": vitest 5 no longer depends on `@vitest/expect`, and runner is
  deprecated). `vitest-browser-svelte@3.1.0(...)(vitest@5.0.5)`. `playwright@1.64.1`/`playwright-core@1.64.1`
  entries are unchanged, so the devloop image needs no rebuild for this diff. Every new entry has a registry
  `integrity`; no tarball/git/file sources. `pnpm install` under `strictDepBuilds: true` passed, so no new install
  script needs an `allowBuilds` entry. Other transitive churn comes from re-resolving coverage-v8's deps
  (`@babel/parser` 7.29.9 etc.).

### OTel duplicate registration (root cause, measured)
**Why the first count was vacuous.** Under an agent, vitest (4 and 5) picks the `agent`/minimal reporter
(`vitest/dist/chunks/defaults.*.js`: `reporters: [isAgent ? "minimal" : "default"]`), and that reporter prints no
console output for passing tests. On v4, browser console still reached the log through Vite 8's
`server.forwardConsole` (on by default under agents; this is the `[vite] (client) [console.error]` line
@observability's control saw). `@vitest/browser` 5 sets `forwardConsole: false` (`dist/index.js`: "Vitest forwards
browser console logs ... through its own RPC"). A passing test's `console.error` marker is therefore invisible
under the agent reporter in sdk-core node (v4 and v5) and in the v5 browser tier. CI is not an agent, so it uses
`default`, which prints them. Every count below is measured with `--reporter=default` (CLI, measurement only), the
same output CI produces.

**Positive control (v5, browser, `--reporter=default`):** a throwaway sdk-svelte test doing
`diag.setLogger(DiagConsoleLogger)` plus two `metrics.setGlobalMeterProvider` calls prints
`Error: @opentelemetry/api: Attempted duplicate registration of API: metrics` under
`stderr | src/__tests__/zzScratchControl.test.ts > ...`. A plain `console.error` marker shows the same way. The file was deleted after the run.

**Counts, `grep -c 'Attempted duplicate registration'`, `--reporter=default`:**

| Tier | 4.1.10 | 5.0.5 |
|------|--------|-------|
| sdk-core unit | **16** | **16** |
| sdk-core component, test-utils, web-app node | 0 | 0 |
| sdk-svelte browser (27) | 0 | 0 (the 1 in the raw log was the control file, removed) |
| web-app browser (113) | 0 | 0 |

The 16 per file, identical on both versions:
- `signaling-client.test.ts`: 5 tests × {metrics, trace}
- `meeting-session.test.ts`: 1 test × {metrics, trace}
- `telemetryConfig.test.ts`: 4 tests × {context}

**Root cause (the seam @observability predicted).** `resetTelemetryForTest()` (`telemetryConfig.ts:357`) disables
only `diag` and `propagation`. It never disables the `metrics`, `trace` or `context` globals that
`configureTelemetry` registers (`setGlobalMeterProvider`, `tracerProvider.register()`). Each file patches part of
the gap in its own `afterEach`:
- signaling-client, meeting-session and media-transport add `context` + `propagation`, but not metrics/trace. So their second configure is a duplicate on {metrics, trace}.
- telemetryConfig.test.ts adds `metrics` + `trace`, but not `context`. So its duplicate is on {context}.
The pattern matches exactly. #78's duplicate-registration lines are this pre-existing noise, which CI prints and the agent reporter hides. They were not caused by the skew: the #78 shape reproduced here (sdk-core unit, `--reporter=default`) gives the same 16 lines, measured, alongside the skew errors. I can't read #78's own CI log here, so I haven't compared its count line by line.

**One-instance check:** `readlink -f packages/{sdk-core,sdk-svelte,web-app}/node_modules/@opentelemetry/api` all
resolve to `node_modules/.pnpm/@opentelemetry+api@1.9.1/node_modules/@opentelemetry/api`. The lockfile has one
`@opentelemetry/api` version. `experimental.openTelemetry` is set in no config, and stays unset.

### strictPeerDependencies evaluation (Lead / DRY / Ops ask)
Every vitest-family package peers the others at an EXACT version: `@vitest/browser`, `-browser-playwright` and
`coverage-v8` 5.0.5 each peer `vitest: 5.0.5`, and `vitest` peers them back at `5.0.5`. So skew is an unmet peer.
- (a) Positive control, #78 shape + `strictPeerDependencies: true`, `pnpm install`: **fails**, exit 1:
  ```
  [ERR_PNPM_PEER_DEP_ISSUES] Unmet peer dependencies
  packages/sdk-core
    ✕ unmet peer @vitest/browser-playwright  Installed: 4.1.10  Wanted: 5.0.5: vitest@5.0.5
    ✕ unmet peer @vitest/coverage-v8         Installed: 4.1.10  Wanted: 5.0.5: vitest@5.0.5
    ✕ unmet peer vitest                      Installed: 5.0.5   Wanted: 4.1.10: @vitest/coverage-v8@4.1.10
  packages/sdk-svelte
    ✕ unmet peer vitest                      Installed: 5.0.5   Wanted: 4.1.10: @vitest/browser@4.1.10, @vitest/browser-playwright@4.1.10
  ```
  Without the setting, the same install succeeds with only `[WARN] Issues with peer dependencies found` (this is
  how #78 went green at install).
- (b) Clean: `pnpm install` + `pnpm peers check` report "No peer dependency issues found" on the current tree
  (4.1.10) and on the 5.0.5 tree, both with strict on. No unrelated mismatches, so no `allowedVersions` entries.
- **Limit (measured):** with a committed skewed lockfile, `pnpm install --frozen-lockfile` + strict **passes**
  (exit 0, "Lockfile is up to date, resolution step is skipped"). The setting fires only where resolution runs:
  Dependabot's updater, a local `pnpm install`/`pnpm add`, a lockfile re-resolve. CI's frozen install does not
  re-check. `pnpm peers check` does: exit 1 on the skewed tree, exit 0 on both clean trees.
  Lead ruling: include `pnpm peers check` in `scripts/lang/ts/compile.sh` (Plan step 7).
- It also covers root vs sdk-core `@vitest/coverage-v8` drift: coverage-v8 5.0.5 peers `vitest: 5.0.5` exactly.
- It does NOT cover `@opentelemetry/api`: `@opentelemetry/core` peers api as a range (`>=1.0.0 <1.10.0`). That is why
  api goes in the catalog instead.
- Why #78 was partial (Dependabot's `npm-vitest` group covers `vitest` + `@vitest/*`): not determinable here
  without #78's body or Dependabot log. With strict on, the updater's resolution would have failed instead of
  opening a skewed PR.

### Plan
1. Bump in place (pin style kept): root `@vitest/coverage-v8` `^5.0.5`; sdk-core `@vitest/coverage-v8` `5.0.5`,
   `vitest` `^5.0.5`; sdk-svelte + web-app `@vitest/browser`/`@vitest/browser-playwright` `5.0.5`, `vitest`
   `^5.0.5`; test-utils `vitest` `^5.0.5` (all 9 entries). `vitest-browser-svelte` stays `^3.1.0`.
2. `@opentelemetry/api` catalog: `catalog: { '@opentelemetry/api': 1.9.1 }` in `pnpm-workspace.yaml`. sdk-core's
   `dependencies` entry and new sdk-svelte/web-app **devDependencies** use `"catalog:"`.
   - **Lead ruling:** within the pinning-style constraint, because the value stays exact and all three packages are private.
   - **Comment (narrow scope, per @dry-reviewer/@code-reviewer; the inclusion criterion mirrors `SINGLE_VERSION_CRATES` in
     `crates/dt-guard/src/workspace_deps.rs`). Draft for the `catalog:` block:**
     ```yaml
     # catalog: a dependency that holds a PROCESS-GLOBAL singleton and so must resolve to ONE copy across every
     # importer, where strictPeerDependencies cannot enforce that (its dependents peer it as a range). Membership is
     # that criterion, not "shared versions live here": typescript, vite, @types/node etc. are duplicated across
     # manifests but hold no global, so they stay as plain pins.
     #   @opentelemetry/api — the OTel API registry (Symbol.for('opentelemetry.js.api.1')); @opentelemetry/core peers
     #                        it as >=1.0.0 <1.10.0. sdk-svelte and web-app re-declare it ONLY because their test
     #                        bundles compile sdk-core SOURCE and @vitest/browser 5 excludes it from optimizeDeps, so
     #                        it must resolve from their own node_modules.
     # The vitest family is NOT here: it cross-peers at exact versions, so strictPeerDependencies enforces lockstep.
     ```
   - **In summary:** an entry here is a dependency holding a
     process-global singleton that must resolve to ONE instance across importers and that strict peers cannot
     enforce (`@opentelemetry/core` peers api as a range). Today that is only `@opentelemetry/api`. sdk-svelte and
     web-app re-declare it only because their test bundles compile sdk-core source and `@vitest/browser` 5
     excludes it from pre-bundling. The vitest family is NOT here: its exact cross-peers plus
     `strictPeerDependencies` enforce it.
   - **Justification by drift hazard in general:** a split version means the one shared
     `Symbol.for('opentelemetry.js.api.1')` registry rejects a mismatched copy ("does not match"), or
     `getGlobal`'s compatibility gate silently returns no-op providers. The catalog is not offered as an
     explanation of #78.
   - **Dependabot:** the npm updater has supported pnpm catalogs since 2025-02-04 (GA:
     https://github.blog/changelog/2025-02-04-dependabot-now-supports-pnpm-workspace-catalogs-ga/, cited by
     @operations). `@opentelemetry/api` still matches the `npm-otel` group's `@opentelemetry/*` pattern. Whether
     the catalog entry is bumped *in the same group PR* as its siblings is per that changelog, not verified here
     by a run. Fallback signal if it lags: `pnpm outdated` / audit. Nothing else would catch a stale api.
   - **The catalog does not enforce itself.** A hand-written `"1.9.2"` in a 4th manifest satisfies every `^1.x`/range
     peer and installs a second copy, and neither strict peers nor `pnpm peers check` sees it. Recorded on the 5.0.5
     tree: `pnpm why -r @opentelemetry/api` → `@opentelemetry/api@1.9.1` ← sdk-core (dependencies), sdk-svelte
     (devDependencies), web-app (devDependencies), core/sdk-*/exporters, vitest — "Found 1 version of
     @opentelemetry/api". (`catalogMode: strict` was considered. It only governs `pnpm add`, not hand edits, so it was not taken.) A TS analogue of dt-guard's R1/R4
     (re-declared outside the catalog / duplicated in lockfile) is new dt-guard machinery (infrastructure; policy
     @observability). It goes into `docs/TODO.md` naming both owners. I have no <10-line fit in an existing guard.
3. `toHaveTextContent` (the mechanism: v4 semantics were substring for strings and match for RegExp; v5 is exact).
   All 38 uses across `packages/*/src` (no other package has any; grep), enumerated:
   - **negated (1):** `web-app/src/__tests__/joinMeeting.test.ts:67`.
   - **converted to `toMatchTextContent` (8):** `web-app/.../inMeeting.test.ts:336,361,374,428`,
     `web-app/.../joinMeeting.test.ts:66,148,176`, `web-app/.../authViews.test.ts:109` (RegExp).
   - **kept `toHaveTextContent`, now exact (29):** `web-app/.../inMeeting.test.ts:116,128,150,157,162,174,188,193,435`;
     `web-app/.../createMeeting.test.ts:68`; `web-app/.../joinMeeting.test.ts:49,54`;
     `sdk-svelte/src/__tests__/bindMeetingSession.test.ts:17,20,23,26,44`;
     `sdk-svelte/src/__tests__/MediaStore.test.ts:185,188,191,197,200,211,234,240,242,257,269,275`.
   Rules:
   - every **negated** use → `.not.toMatchTextContent` (only `joinMeeting.test.ts:67`). Red demo with the ORIGINAL
     leak shape: the rendered error contains `SECRET_TOKEN_123` as a substring of a longer message → red; output
     pasted; revert. Line 66's positive check is kept, so :67 cannot pass on an empty error;
   - the 8 positives that rely on substring/RegExp semantics (`inMeeting.test.ts:336,361,374,428`,
     `joinMeeting.test.ts:66,148,176`, `authViews.test.ts:109`) → `toMatchTextContent` with the same argument;
   - positive uses that pass under strict equality stay `toHaveTextContent` and now assert more than before
     (`'muted'` no longer matches `'unmuted'`).
4. `.gitignore`: replace `.vitest-attachments/` and `**/__tests__/__screenshots__/` with `.vitest/`. The guide
   (§Generated Reports and Artifacts) moves *failure screenshots* to `.vitest/attachments/failure-screenshots/`
   and attachments to `.vitest/attachments/`. This was observed in the 5.0.5 run (`Failure screenshot:
   - .vitest/attachments/failure-screenshots/...`). Nothing else writes `__screenshots__` (no `toMatchScreenshot`).
   Guide citation: §"Generated Reports and Artifacts Use the `.vitest` Directory": "Failure screenshots move to
   `.vitest/attachments/failure-screenshots/`". The commit message carries a cleanup line for leftover old dirs, plus "remove packages/*/node_modules/.vite after
   pulling" (@test).
5. **Root-cause fix for the pre-existing OTel duplicate registration.** This is a pre-existing defect surfaced during
   the migration, NOT a vitest 5 breaking change. The commit message names it that way. Lead ruling: shut down the providers too.
   - `resetTelemetryForTest()` becomes `async` and returns `Promise<void>`. It clears `configured`, then disables every
     global `configureTelemetry` registers (`diag`, `metrics`, `trace`, `context`, `propagation`), then awaits
     `meterProvider.shutdown()` and `tracerProvider.shutdown()` of the previous config. The shutdown stops the
     leaked `PeriodicExportingMetricReader` timer.
   - Callers (all in sdk-core tests: `telemetryConfig`, `signaling-client`, `meeting-session`, `media-transport`)
     use `afterEach(async () => { await resetTelemetryForTest(); ... })`. Their now-redundant
     `metrics/trace/context/propagation.disable()` lines are removed (SSoT). `tracePropagation.test.ts` never calls
     the reset and keeps its own disables. It is not consumed outside sdk-core (grep: sdk-svelte and web-app have no calls).
   - Required test-double change: the `OTLPMetricExporter` mock in `telemetryConfig.test.ts` gains no-op `export`,
     `forceFlush` and `shutdown`. Without them, `MeterProvider.shutdown` throws
     `TypeError: this._exporter.forceFlush is not a function` (measured: 7 failures in a prototype without it).
   - **Regression test** (`telemetryConfig.test.ts`, `describe('resetTelemetryForTest')`): it installs a spy `DiagLogger` at
     ERROR, then runs `configureTelemetry({env:'production'})` → reset → re-set spy → configure again. It asserts that no
     error message contains "duplicate registration" and that the tracer is defined. `env:'production'` stops
     configure replacing the spy with a console logger. **Red on today's helper (prototype on a scratch copy):**
     ```
     × lets configureTelemetry run again with no duplicate global registration
     + "Error: @opentelemetry/api: Attempted duplicate registration of API: context
     + "Error: @opentelemetry/api: Attempted duplicate registration of API: metrics
     + "Error: @opentelemetry/api: Attempted duplicate registration of API: trace
     ```
     This test does not depend on reporter or console visibility. It is the durable guard.
   - **Prototype result (scratch, `--reporter=default`):** sdk-core unit has 70 files and 1029 tests (1028 plus the
     new regression test), all pass. Coverage is unchanged at 96.63 / 90.99 / 96.07 / 97.76. Duplicate-registration
     lines went from 16 to **0**. There are no `TypeError`/unhandled errors, and the real-exporter tests show no export
     error on shutdown. The final per-file × API counts for both trees are re-measured after implementation.
   - **Resolved (Lead ruling: option A, later tier-wide via operator-approved `setupFiles`): shutdown caused real egress.** Probe on scratch, local HTTP server: configure +
     record one counter + `await reset` → exactly one `POST /t/v1/metrics` (no `authorization` header); configure +
     reset with nothing recorded → 0. Real-exporter callers: `signaling-client.test.ts` (6 sites),
     `meeting-session.test.ts:698`, `media-transport.test.ts:174`, all `https://gc.example`, no `authTokenProvider`.
     The prototype disabled `diag` before shutdown, which made such an export failure silent. The order will be
     shutdown first, then disable `diag`. Proposed (A): one shared no-op OTLP exporter mock for those 3 files, so the
     unit tier is fully offline. Alternative (B): drop shutdown and only disable globals. Today's leaked reader already
     has the same egress latent: a 10 s unref'd interval.
   - Production `configureTelemetry` replace-semantics stay out of scope (filed in docs/TODO.md §Observability Debt).
6. `strictPeerDependencies: true` in `pnpm-workspace.yaml`. The comment block, in the file's style, says what it
   stops (PR #78's family skew), that it relies on the vitest family's exact-version peers, that it does not hold
   under `--frozen-lockfile` (hence 7), and that it is stated explicitly so the policy does not depend on a default.
7. (Lead ruling: include) `run_and_emit "pnpm-peers" pnpm peers check || exit 1` in `scripts/lang/ts/compile.sh`,
   after `pnpm_deps_fresh`, before `nx-typecheck`.
   - Wrapper contract: `_dispatch.sh` takes each wrapper's LAST `STATUS=` line, so the check must abort on failure
     (`|| exit 1`, the same shape as `pnpm_deps_fresh || exit 1`). On success its OK line precedes nx-typecheck's,
     and the layer's `tee_collect_statuses` worst-wins over both.
   - No token allow-list or self-test names TS compile tokens: `grep nx-typecheck` hits only `compile.sh` and the
     runbook.
   - Reaches CI: `.github/workflows/ci.yml:47-48` (shard `guards`, layers `1,2,3,6,7`) → `ci.yml:134`
     `pnpm install --frozen-lockfile` → `ci.yml:241` `./scripts/layer-all.sh --layers` → Layer 1 → `compile.sh`.
   - Positive control through the wrapper: `compile.sh` on the committed-skew tree must red with
     `STATUS=FAIL REASON=pnpm-peers-failed`, and be green on 5.0.5. STATUS lines will be pasted.
   - Runbook: add `pnpm-peers-failed` to the `docs/runbooks/devloop-validation.md` token table, and the OK line to
     the Layer-1 stage-2 worked examples.
8. Stale prose: "Vitest 4 Browser Mode" → version-free in `packages/{sdk-svelte,web-app}/vitest.config.ts`,
   `MeetingStore.test.ts:1`, `docs/runbooks/client-dev-local.md:1721`. The `docs/TODO.md` `vitest-coverage-v8-tooling`
   entry is rewritten per @test's (a)–(c): drop "not installed", "blocks R-41" and "vitest ^4.1.8"; state that the
   sdk-core 90% gate is live; the residual is that test-utils declares a v8 block but `test:unit` has no
   `--coverage`; owner test; pickup is `--coverage` plus a threshold decision.
9. Evidence after implementation (browser tiers via the scratch browser path, labelled):
   - per-tier counts + the sdk-core coverage line;
   - **unconditional** sorted test-name diff vs baseline (JSON reporter, every tier; baseline taken from a v4
     worktree);
   - each tier once with `clearMocks: false` (scratch config only, never committed): same green counts;
   - duplicate-registration counts per file × API with `--reporter=default`, on the v4 baseline (16) and the
     final tree (target 0);
   - **every** console-grep measurement (duplicate registration, "was not awaited", any "no warnings" claim) uses
     `--reporter=default`. The un-awaited grep runs over every tier's v5 log under it, with the positive control
     re-run under it. (In v5 an un-awaited assertion *fails* the test, so it also shows as red in any reporter.);
   - `grep 'was not awaited'` over every tier's v5 log. The v5 text is `Error: expect.element(locator)....() was not
     awaited` / `Promise returned by \`expect(actual).resolves...\` was not awaited`, and it FAILS the test.
     Positive control already run (scratch test with an un-awaited `expect.element` and `.resolves` → both failed with
     that text, removed);
   - `./scripts/layer-fast.sh` with no browser env var. If the component lane reds on the missing Chromium, that
     goes to the Lead as a precondition failure.
10. Rollback / delivery: ONE commit (manifests, lockfile, catalog, strict peers, compile.sh lane + runbook row,
    .gitignore, test edits, reset fix, comments, TODO). No CI-workflow edits. Rollback = `git revert` +
    `pnpm install --frozen-lockfile` + `rm -rf packages/*/node_modules/.vite` (a revert is a dependency change too;
    a stale optimizer cache produced 4 phantom joinMeeting failures in this loop's baseline). A revert returns `@opentelemetry/api` to sdk-core only, which is correct for
    v4. Remaining actions: (1) host `infra/devloop/devloop.sh --rebuild` (pre-existing; docs/TODO.md §Developer
    Experience); (2) close Dependabot #78 after merge. CI note: `ci-client.yml`'s Playwright cache key hashes
    `pnpm-lock.yaml`, so the first run is cold (expected).

**For future devloops:** nothing in this repo may rely on agent-shell console visibility. Under `AI_AGENT`/`CLAUDECODE`,
vitest's minimal reporter hides passing tests' console output (`--silent=false` does not override it on v4). Any
console-based evidence must be taken with `--reporter=default`.

**Reviewed against the guide, options we use: no config edit needed.**
- coverage `include`/`exclude` (§Coverage include and exclude Match More Precisely): same 78-file set, same %.
- `browser.provider` / `instances` / `headless` / `launchOptions`: API unchanged, args carried over unchanged.
- `retry: 0`, thresholds 90: unchanged.
- Not used: `projects`/workspace (§4–6), `browser.api`/`browser.isolate` (§29), `screenshotDirectory`/`toMatchScreenshot`
  (§31), `testNamePattern` (§3), json/junit/html reporters (§30), benchmarks (§10), `VITEST_POOL_ID`/`WORKER_ID` (§32),
  `toThrow('')` (§13), deprecated entrypoints (§35).
- Configs are found without parent lookup (§25): every script runs from its package dir or passes `--config`.
- Locators exact (§20): inert, since only `getByTestId` is used.
- `clearMocks` default true (§2): **corrected reasoning (@test):** clearing history before each test can make
  negative and upper-bound call assertions (`not.toHaveBeenCalled`, `toHaveBeenCalledTimes(n)` as a ceiling)
  WEAKER, because a call at module scope or in `beforeAll` no longer counts. All tiers pass with
  `clearMocks: false` (scratch run, same counts; web-app browser shows only the same 8 pre-fix toHaveTextContent
  failures). So no assertion depends on the new default in either direction. No config change.
- Un-awaited assertions (§16): method and control as in step 9.

**Breaking changes / upstream behaviour changes acted on** (for the commit message):
1. `@vitest/browser` 5 always excludes `@opentelemetry/api` from optimizeDeps (upstream source, not in the guide).
   Required: declare it in sdk-svelte and web-app (devDependencies), versioned via the new catalog.
2. `toHaveTextContent` is strict equality and takes no RegExp (§21). Required: 9 assertions → `toMatchTextContent`,
   one of them a negated credential-leak check that had gone vacuous.
3. Artifacts move to `.vitest/` (§30). Required: `.gitignore`.
4. `@vitest/browser` 5 disables Vite's `forwardConsole`. Required: nothing in config. It is the reason the
   duplicate-registration measurement had to use `--reporter=default`; it led to the reset fix (step 5), which
   addresses a pre-existing bug, not a v5 break.
5. Package migration (§34): `@vitest/{expect,runner,snapshot}` leave the graph, and `@vitest/ui` arrives as a dependency of
   `@vitest/browser`. Required: nothing (no direct imports, no `--ui`).
6. Test titles use pretty-format (§17). Required: nothing. Two `auth-api-client.test.ts` titles lose the quotes
   around `$`-interpolated strings, which shows up as 2 renames in the test-name diff.

**Operator-approved exception, not a guide change:** `packages/sdk-core/vitest.config.ts` gains
`setupFiles: ['src/telemetry/__tests__/otlpExporterSetup.ts']`. The task allows config edits only where the migration guide
requires them, and no guide section requires this. It is part of the telemetry-reset fix below. `resetTelemetryForTest` now shuts the
providers down, which exports to the configured endpoint, so the exporter mock has to be tier-wide by construction (@security's
review finding) rather than a per-file opt-in a new test can forget (measured: a forgotten mock = a silent real POST,
23 connects in the red demo). The operator approved keeping it.

Not a vitest 5 breaking change. Pre-existing defect surfaced during the migration: `resetTelemetryForTest()`
reset only 2 of the 5 OTel globals, which caused 16 duplicate-registration errors in sdk-core on v4 and v5 alike.
These were hidden locally by the agent reporter. Fixed at the helper, with a red-first regression test.

Security asks, confirmed: exact pins stay exact; `allowBuilds`, `strictDepBuilds`, `overrides`, audit suppressions
untouched; no `server.host`/`api.host`/`allowedHosts`/CORS; `launchOptions.args` unchanged; no `--ui`/`browser.ui`.
`bundle-content.test.ts`, `telemetryOrigin.test.ts` and the frame/sframe vector suites are in the measured counts
and the name diff.
`@vitest/coverage-v8` twice (root `^`, sdk-core exact): both move. sdk-core's is the operative one (the only
package running `--coverage`). The root entry stays per the task. Strict peers plus `pnpm peers check` fail on
either one skewing, because coverage-v8 peers `vitest` exactly.

---

## Pre-Work

None

---

## Accepted Deferrals

None.

---

## Implementation Summary

| Path | Why |
|------|-----|
| `package.json` | root `@vitest/coverage-v8` `^4.1.10` → `^5.0.5` (caret kept) |
| `packages/sdk-core/package.json` | `@vitest/coverage-v8` `5.0.5` (exact kept), `vitest` `^5.0.5`; `@opentelemetry/api` → `catalog:` |
| `packages/sdk-svelte/package.json` | `@vitest/browser` + `-playwright` `5.0.5`, `vitest` `^5.0.5`; new devDependency `@opentelemetry/api: catalog:` |
| `packages/test-utils/package.json` | `vitest` `^5.0.5` |
| `packages/web-app/package.json` | as sdk-svelte |
| `pnpm-lock.yaml` | `pnpm install` only |
| `pnpm-workspace.yaml` | `strictPeerDependencies: true` + comment; `catalog:` (`@opentelemetry/api: 1.9.1`) + narrow-scope comment |
| `packages/sdk-core/src/telemetry/telemetryConfig.ts` | `resetTelemetryForTest` async: shuts down the previous providers, then disables metrics/trace/context/propagation/diag (root cause of the pre-existing duplicate registration) |
| `packages/sdk-core/src/telemetry/__tests__/otlpExporterMock.ts` | NEW: the one no-op OTLP exporter mock (export/forceFlush/shutdown + `exporterCtor`, `exportCalls`) |
| `packages/sdk-core/src/telemetry/__tests__/otlpExporterSetup.ts` + `packages/sdk-core/vitest.config.ts` | NEW `setupFiles` entry: the exporter mock is installed tier-wide, so offline holds by construction, not per-file opt-in (@security review finding) |
| `packages/sdk-core/src/telemetry/__tests__/telemetryConfig.delta.test.ts` | `vi.unmock` of the exporter: it asserts the REAL exporter (temporality, header re-ask). Without the opt-out its 7 tests fail loudly under the tier-wide mock (measured) |
| `packages/sdk-core/src/telemetry/__tests__/telemetryConfig.test.ts` | uses the shared mock; async reset; drops its own metrics/trace disables; new reset regression test |
| `packages/sdk-core/src/{signaling,session,media}/__tests__/*.test.ts` (3 files) | `await resetTelemetryForTest()`; drop now-redundant context/propagation disables and unused imports; media-transport adds the mock-wiring positive control |
| `packages/web-app/src/__tests__/{inMeeting,joinMeeting,authViews}.test.ts` | 9 assertions `toHaveTextContent` → `toMatchTextContent` (guide §21) |
| `packages/{sdk-svelte,web-app}/vitest.config.ts`, `MeetingStore.test.ts`, both READMEs, `docs/runbooks/client-dev-local.md` | "Vitest 4 Browser Mode" → version-free (comment/prose only) |
| `scripts/lang/ts/compile.sh` | `run_and_emit "pnpm-peers" pnpm peers check \|\| exit 1` before typecheck (Lead ruling); `--output-style=static` on nx typecheck |
| `scripts/lang/_pnpm.sh` | `NX_PIPELINE_OUTPUT_STYLE` — the one home of the pipeline nx output style + rationale (D-1 / CR-F4) |
| `scripts/lang/ts/{fmt,lint,test}.sh` | `--output-style=static` on the CHECK-mode format / lint / test nx calls (Lead ruling, Issue 3): nx 23.3 hides failing-task output under an AI agent |
| `docs/runbooks/devloop-validation.md` | `pnpm-peers-failed` token row + OK line in both Layer-1 worked examples |
| `.gitignore` | v4 artifact dirs → `.vitest/` (guide §30) |
| `docs/TODO.md` | `vitest-coverage-v8-tooling` rewritten to its residual (test-utils); new §DRY entry for the catalog self-enforcement guard (infrastructure + observability). The other two new entries in the diff (configureTelemetry semantics, stale-image precondition) were filed by @observability / @operations / Lead, not by me |

---

## Implementation Notes

All browser-tier numbers below were measured with `PLAYWRIGHT_BROWSERS_PATH=<scratch>/pw` (chromium_headless_shell-1248),
on the measuring command only. That is a measurement aid for the stale devloop image; it is not in any tracked file or script, and not in
layer-fast. Every console-based count uses `--reporter=default`.

**Install.** `pnpm install` produced no `minimumReleaseAgeExclude` and no `allowBuilds` change; `strictDepBuilds` passed.
`pnpm peers check` → "No peer dependency issues found"; `pnpm why -r @opentelemetry/api` → "Found 1 version". No
4.x vitest-family entry is left in any manifest or the lockfile. pnpm reported that the shared store at `~/.local/share/pnpm/store/v11` can't be hard-linked into
/work, so it fell back to `node_modules/.pnpm-store` (gitignored). That's an environment note, not a diff.

**Per-tier counts, final tree vs the v4 baseline (wt at `2d412583`, same reporter):**

| Tier | v4 baseline | final | dup-registration v4 → final | "was not awaited" |
|------|-------------|-------|-----------------------------|-------------------|
| sdk-core unit | 70 / 1028 | 70 / **1032** | **16 → 0** | 0 |
| sdk-core component | 1 / 4 | 1 / 4 | 0 → 0 | 0 |
| test-utils unit | 7 / 19 | 7 / 19 | 0 → 0 | 0 |
| sdk-svelte browser | 4 / 27 | 4 / 27 | 0 → 0 | 0 |
| web-app node | 11 / 224 | 11 / 224 | 0 → 0 | 0 |
| web-app browser | 13 / 113 | 13 / 113 | 0 → 0 | 0 |

sdk-core coverage: All files 96.63 / 90.99 / 96.07 / 97.76, the same as baseline. It's still emitted only by sdk-core unit. 0 skipped anywhere.

**Unconditional test-name diff** (JSON reporter, every tier, sorted `(file, fullName, status)`):
- sdk-core unit: +4 new tests, all from the reset fix. In `telemetryConfig.test.ts > resetTelemetryForTest`: `lets
  configureTelemetry run again with no duplicate global registration`, `a rejecting exporter shutdown rejects the reset`
  and `a failed final export during the reset reaches diag`. In `media-transport.test.ts`: `telemetry in this file never
  reaches a real exporter resetTelemetryForTest's shutdown export lands on the shared mock`. (sdk-core name diff re-run at 1032: exactly
  these 4 added + the 2 renames below, nothing removed.)
- sdk-core unit: 2 **renames, not swaps**: `auth-api-client.test.ts` titles
  `maps register 400 'INVALID_REQUEST' to 'AuthBadRequestError'` → `... 400 INVALID_REQUEST to AuthBadRequestError` (and the same for
  409/CONFLICT). Guide §17 "Test Titles ... Use pretty-format": `$`-interpolated strings in titles are no longer
  quoted. Same test, same status.
- Every other tier: identical name sets.

**clearMocks.** Post-fix, every tier re-run with `clearMocks: false` (scratch mirror only, configs restored) gives the same
counts (1030/4/19/27/224/113, all green).

**Un-awaited assertions.** 0 `was not awaited` in every tier's `--reporter=default` log. Positive control (scratch): an
un-awaited `expect.element(...)` and an un-awaited `.resolves` each FAIL with
`Error: expect.element(locator).toBeInTheDocument() was not awaited ...` /
`Promise returned by \`expect(actual).resolves.toBe(expected)\` was not awaited ...`.

**Red demo, joinMeeting.test.ts:67 (substring leak shape, on the scratch mirror, reverted):** the error message was changed to
`'join rejected for SECRET_TOKEN_123 holder'`.
```
× surfaces an error via last-error without leaking a non-allowlisted field
Error: expect(element).not.toMatchTextContent()
Expected element not to match text content:
  SECRET_TOKEN_123
Received:
  SIGNALING: join rejected for SECRET_TOKEN_123 holder
```
The same leak, with the v5 `.not.toHaveTextContent` (the unfixed line), gives `✓ 1 passed`. That is the vacuity this fixes.

**Red demo, reset regression test on the OLD helper** (HEAD's `telemetryConfig.ts` on the mirror, restored):
```
× lets configureTelemetry run again with no duplicate global registration
+   "Error: @opentelemetry/api: Attempted duplicate registration of API: metrics ...
+   "Error: @opentelemetry/api: Attempted duplicate registration of API: trace ...
```
It's green with the new helper.

**Red demo, mock wiring positive control:** with media-transport's `vi.mock` line removed (mirror, restored), the test fails:
`× resetTelemetryForTest's shutdown export lands on the shared mock` (the real exporter's export never reaches `exportCalls`).
It's green with it. Network backstop question (@security): sdk-core has no global network trap. msw `onUnhandledRequest:
'error'` is per-file, in the two `http/__tests__` API-client files only, with no `setupFiles`. So nothing would have caught
the shutdown egress, and the per-file exporter mock plus this control are what keep it offline.

**Failure visibility through the reset (Lead condition 1).** Order: shut down the providers, then disable
metrics/trace/context/propagation, with `diag` LAST. Measured contract (sdk-metrics 2.12): a rejecting exporter `shutdown()`
**rejects** `resetTelemetryForTest()`; a failed final export inside the shutdown flush is **reported to `diag`**
(`PeriodicExportingMetricReader: metrics export failed ...`), not thrown. There is one test for each. Red demo, with `diag.disable()`
moved before the shutdown (mirror, restored):
```
× a failed final export during the reset reaches diag
AssertionError: expected '' to contain 'export boom'
```
Green with the shipped order.

**Offline positive control (Lead condition 3).** I ran a network spy preloaded into every vitest worker
(`NODE_OPTIONS=--import <scratch>/netspy/spy.mjs`). It records `net.Socket#connect` (non-IPC), `dns.lookup` and `fetch` to a file.
Spy control: a scratch test doing one `https.request` and one `fetch` to gc.example is recorded (2 connect, 2 dns, 1 fetch).
Whole sdk-core unit tier:

| Tree | outbound attempts |
|------|-------------------|
| start commit `2d412583` (old helper, no shutdown) | **1** × `connect gc.example:443` (+ dns). A leaked reader's 10 s interval fired inside a long file |
| new helper, the three `vi.mock` lines removed (mirror) | **14** × `connect gc.example:443` (+ 14 dns). This is the shutdown flush egress (A) prevents; the mock-wiring control also fails |
| final tree | **0** records |

The probe from planning (local HTTP server, configure + record + await reset → exactly one `POST /t/v1/metrics`, no
`authorization` header) is the single-call form of the middle row.

**Offline by construction (@security review finding).** The mock is installed by
`src/telemetry/__tests__/otlpExporterSetup.ts` (`setupFiles` in `packages/sdk-core/vitest.config.ts`), not by
per-file `vi.mock` lines. Those four lines are removed. The only real-exporter file, `telemetryConfig.delta.test.ts`, opts out
with `vi.unmock`. Measured: without it, all 7 of its tests fail under the tier-wide mock, so a forgotten opt-out is loud,
not vacuous. With the setup file, sdk-core is 70 / 1032, coverage is unchanged, duplicate registration is 0, and the network spy shows **0** records.
**Red demo, `setupFiles` entry removed (restored after):** 4 tests fail (the media-transport wiring control, the exporter-ctor
assertion, and both failure-visibility tests), and the spy records **23 × `connect gc.example:443`**. So the media-transport
control now proves the tier-wide wiring. It is the one file with no mock of its own and no opt-out.

**Shared mock location (Lead condition 2):** `packages/sdk-core/src/telemetry/__tests__/otlpExporterMock.ts`. That's sdk-core's
test-only tree: it's excluded from coverage by `src/**/__tests__/**`, and it's not a `*.test.ts`, so it isn't collected. It was first wired with a one-line per-file `vi.mock`.
That was superseded by the tier-wide setup file above. There are no per-file copies or mocks. `telemetryConfig.test.ts`'s old inline mock was replaced by it too.

**Cleanup note for the commit message (Lead #3):** after pulling, remove leftover v4 artifact dirs
`packages/*/.vitest-attachments/` and `packages/*/src/__tests__/__screenshots__/` (no longer ignored; vitest 5 writes
`.vitest/`), and `packages/*/node_modules/.vite` (the dependency change invalidates the optimizer cache).

**`pnpm peers check` lane.** It works offline: `unshare -rn pnpm peers check` → exit 0, "No peer dependency issues found".
Positive control through the wrapper:
```
# #78-shape tree (vitest ^5.0.5, @vitest/* 4.1.10, lockfile committed from a non-strict install, frozen install)
$ scripts/lang/ts/compile.sh   → exit 1
STATUS=FAIL REASON=pnpm-peers-failed
# final tree
$ scripts/lang/ts/compile.sh   → exit 0
STATUS=OK REASON=pnpm-peers-passed
STATUS=OK REASON=nx-typecheck-passed
```

**`resetTelemetryForTest` callers all await it:** `grep 'resetTelemetryForTest()' packages --include=*.ts | grep -v await`
leaves only the definition and a doc comment. There is no `no-floating-promises` lint rule in `eslint.config.js`.

---

### Post-review state (Gate 3 fixes; supersedes the 1032 figures above)
- **O-1 / CR-F1 (reset failure path):** `resetTelemetryForTest` now runs `Promise.allSettled` over both shutdowns, then
  ALWAYS disables metrics/trace/context/propagation and finally `diag`, then rethrows the first rejection. The test `a rejecting
  exporter shutdown rejects the reset AND still clears every global` re-configures after the rejection and asserts no
  "duplicate registration". **Red on the previous helper**: `+ "Error: @opentelemetry/api: Attempted duplicate registration
  of API: metrics ...`. Green now. The docstring says the globals are disabled even on failure.
- **Test-F1 (`diag.disable()` unpinned):** new test `removes configureTelemetry's console diag logger`. `DiagConsoleLogger`
  binds the console methods at module load, so a `console.error` spy cannot see it (measured: 0 calls even before the reset).
  The test therefore observes the registry: after the reset, a fresh WARN logger set with `diag.setLogger` must receive no
  warning at all (`expect(warn).not.toHaveBeenCalled()`, so it does not depend on OTel's wording). **Positive control**
  (@test re-review): `control: without a reset, a new logger IS warned about the registered one` asserts that the same sequence
  without the reset DOES warn. A library change that stopped signalling the overwrite reds the control, so the guard can't go
  silently vacuous. Results: shipped → 12/12 green; `diag.disable()` removed → `× removes configureTelemetry's console diag
  logger`, 1 failed | 11 passed.
- **Counts:** sdk-core unit is 70 files / **1034** (1028 + 6 reset-fix tests, including the diag control). Coverage is 96.64 / 91.00 / 96.08 / 97.76 (baseline
  96.63 / 90.99 / 96.07 / 97.76). The +0.01 comes from the new tests exercising the reset's rejection branch; same 78-file set.
  Duplicate registration is 0 and the network spy shows 0 records.
- **setupFiles red demo, re-run after O-1:** with the entry removed there are 4 failures (ctor assertion, rejecting-shutdown,
  failed-export, media-transport wiring control) and the spy records 22 × `connect gc.example:443`. Restored → green, 0 records.
- **D-1 / CR-F4:** the value lives once in `scripts/lang/_pnpm.sh` (`NX_PIPELINE_OUTPUT_STYLE='--output-style=static'`, with
  the rationale). compile/lint/test.sh and fmt CHECK pass `"$NX_PIPELINE_OUTPUT_STYLE"` and keep their literal `nx
  run-many -t ...` calls (`layer7.test.sh:1707` pin). Re-verified: each wrapper prints the failing-task marker 3× under the
  agent env; `fmt.test.sh` 23/23, `_pnpm.test.sh` 16/16, `layer7.test.sh` 513/513.
- **CR-F2 remainder:** the `otlpExporterMock.ts` header points at the setup file. The delta-test opt-out exists. The
  media-transport control comment says it proves the setup-file wiring. **CR-F3:** `@opentelemetry/api` sorted first in
  both devDependency blocks (frozen install: lockfile unchanged). **CR-F5:** one blank line in compile.sh. **CR-F6:**
  `context` sorted first in the telemetryConfig.ts import list. **CR-F7:** the catalog-guard TODO entry (with @dry-reviewer's
  sibling-class paragraph) moved to §Developer Experience next to the pin-guard entries.
- **Ops:** runbook Layer-1 worked example arrow → `OK OK OK FAIL = FAIL`. Rollback adds `rm -rf packages/*/node_modules/.vite`.
- **@test's 2 title renames** are recorded as an explained difference (guide §17), above.

## Issues Encountered & Resolutions

### Issue 1: Devloop image has a stale Playwright browser (host-op)
**Problem**: `/opt/ms-playwright` holds `chromium_headless_shell-1234` (Playwright 1.62.1), but 8fbb6075 bumped `playwright` to 1.64.1, which needs `-1248`. On the unchanged start commit, `test:component` in sdk-svelte and web-app fails at launch: "Executable doesn't exist at /opt/ms-playwright/chromium_headless_shell-1248/...". The directory is root-owned.
**Resolution**: No in-tree fix — `PLAYWRIGHT_VERSION` is already derived from `pnpm-lock.yaml` by `infra/devloop/devloop.sh`. Host action: `infra/devloop/devloop.sh --rebuild` and restart the container. Until then, a scratch `PLAYWRIGHT_BROWSERS_PATH` is allowed only for baseline/after count measurement, labeled as such. It does not satisfy layer-fast or Gate 2.

---

## Gate 1 — Plan Confirmations

| Reviewer | Plan Status |
|----------|-------------|
| Security | confirmed |
| Test | confirmed |
| Observability | confirmed |
| Code Quality | confirmed |
| DRY | confirmed |
| Operations | confirmed |

Gate 1 PASSED (round 3): all six confirmed; `validate-cross-boundary-classification.sh` → `STATUS=OK`. Lead hold (round 1, resolved): plan lacks the `strictPeerDependencies` evaluation (positive control against #78's shape + clean-install check); OTel drift→duplicate-registration claim to be labelled as hypothesis vs per-file measured counts; confirm `@opentelemetry/api` additions are devDependencies only.

### Issue 2: Agent-shell reporter hid the OTel duplicate-registration errors (Lead-verified)
**Problem**: In agent shells (`CLAUDECODE`/`AI_AGENT` set), vitest picks the minimal reporter, which hides console output from passing tests. Every "0 duplicate registration" count measured early in planning was vacuous. Lead reproduction on the start commit (vitest 4.1.10, sdk-core): `vitest run` → 0 lines; `vitest run --reporter=default` → 16. CI uses the default reporter, which is why #78 showed them.
**Resolution**: The error is pre-existing, not caused by vitest 5. Root cause (observability): `resetTelemetryForTest()` resets only `diag` and `propagation`. Fixing the helper is in scope (fix the cause; never silence it). Every console-output measurement in this loop uses `--reporter=default`.

### Issue 3: TS wrappers lose failing-task output under an AI agent (pre-existing; fixed in this diff by Lead ruling)
**Problem**: `scripts/lang/ts/fmt.test.sh` failed 2/23 (`check .ts names the offender`, `check names the .svelte offender`)
in this shell, on the unchanged start commit `2d412583` too. It passes 23/23 with the agent env vars unset. Cause: nx 23.3.0
(bumped in 8fbb6075) resolves its default output style to `summary` when `isAiAgent()`
(`nx/dist/src/command-line/yargs-utils/shared-options.js`: `args.outputStyle ?? (useTui ? 'tui' : isAiAgent() ? 'summary'
: 'static-failures-only')`). Summary prints "Re-run with --output-style=static to inline logs" in place of the
failing task's output. CI is not an agent, so it is unaffected. This is the same class as vitest's agent reporter (Issue 2).

**(a) Correction: the "first run passed" claim was wrong.** The first layer-fast log also shows
`scripts/lang/ts/fmt.test.sh: 21 passed, 2 failed`. Its FAILED_GUARD_NAMES summary named only the scope guard, and I
misread that as the self-test passing. There is no cache effect to explain. On caching: nx does not cache failed tasks, so a
failing task always takes the run path. Measured on a synthetic workspace with `cache: true` on the failing target, the
second run shows no cache hit, and the marker is printed with the flag on both runs.

**(b) Invariant, per wrapper.** A synthetic nx workspace whose `typecheck`/`lint`/`test:unit` targets `echo MARKER_...; exit 1`,
running the real wrappers:

| Wrapper | agent env, no flag | non-agent env, no flag | agent env, `--output-style=static` |
|---------|--------------------|------------------------|------------------------------------|
| `compile.sh` (nx typecheck) | marker **0**, `nx-typecheck-failed` | marker 3 | marker **3** |
| `lint.sh` (nx lint) | marker **0**, `nx-lint-failed` | marker 3 | marker **3** |
| `test.sh` (nx test:unit/component) | marker **0**, `nx-test-failed` | marker 3 | marker **3** |
| `fmt.sh` CHECK (nx format) | offender names absent (self-test 21/23) | 23/23 | **23/23** |
| `fmt.sh` APPLY | already pins `--output-style=stream-without-prefixes` | n/a | unchanged |

**Fix**: `--output-style=static` on all four calls, placed after `--all` and before the caller's `"$@"`. No in-tree caller
passes `--output-style`. `layer7.test.sh`'s `runner-present-ts-l4` substring (`nx run-many -t test:unit`) still matches.
**(c)** `fmt.test.sh` is 23/23 under the agent env with the flag, and 21/23 without it (the start commit and pre-fix /work, each run 3×).

### Issue 4: final `./scripts/layer-fast.sh` (no browser env var), after the Issue 3 fix
- L1 OK, L2 OK, L3 OK (`ts-fmt-selftest` 23/23; scope guard clean).
- **L4 FAIL: browser precondition only (host op, Issue 1).** `nx-test-failed` from `sdk-svelte:test:component` and
  `web-app:test:component`: `browserType.launch: Executable doesn't exist at /opt/ms-playwright/chromium_headless_shell-1248/...`
  ("no tests"). Every other TS tier is green in the same run (1032 / 4 / 19 / 224), and Rust L4 passes. With the scratch
  browser, the two browser tiers are 27/27 and 113/113 (Implementation Notes).
- layer-fast stops at L4, so L5 and L6 were run individually. L5 OK. **L6: `cargo-audit-failed` only** (RUSTSEC-2026-0381/0382/0383,
  rustls-webpki 0.103.15 / webpki-roots / webpki-root-certs 1.0.4; RUSTSEC-2026-0221 event-listener) against the untouched
  `Cargo.lock`. That was at the time of that run. **Superseded**: the operator fixed it in the separate commit 85b12793 (Issue 5);
  this diff makes no Cargo.lock change and adds no suppression. The L6 re-run after 85b12793 is in Issue 6.

### Issue 5: Base-tree Rust advisories block Layer 6 (operator decision: separate commit)
**Problem**: `cargo-audit-failed`: RUSTSEC-2026-0381/0382/0383 (rustls-webpki 0.103.15; webpki-roots and webpki-root-certs 1.0.4, all trusting distrusted certificates). They are in the base tree, and this diff doesn't touch Cargo.lock.
**Resolution**: The operator chose to fix them in a separate commit ahead of the vitest commit: `cargo update -p rustls-webpki -p webpki-roots -p webpki-root-certs` (0.103.16 / 1.0.10 / 1.0.10, 3 packages only). `cargo audit` is now clean apart from the pre-existing allowed warnings. Security reviews it, and it's committed by path only.

### Lead fast check (after Cargo.lock commit 85b12793)
`DEVLOOP_FMT_APPLY=1 ./scripts/layer-fast.sh`: L1 OK, L2 OK, L3 OK, L4 FAIL, with the ONLY failures sdk-svelte and web-app `test:component` (`browserType.launch: Executable doesn't exist at /opt/ms-playwright/chromium_headless_shell-1248/...`, the stale-image host op). All other L4 tiers pass. Run separately: L5 OK, L6 OK (`cargo-audit-passed`, `pnpm-audit-passed`, `buf-breaking-passed`).
Lead decision: start review now. The browser tiers are verified only through scratch-browser measurements, and those don't count for the gate. Gate 2 waits for the host image rebuild; it is not waived.

---

### Issue 6: layer-fast after the Gate 3 fixes (tree stable)
`./scripts/layer-fast.sh` (no browser env var): L1 OK, L2 OK, L3 OK, **L4 FAIL on the browser precondition only**:
`sdk-svelte`/`web-app` `test:component` → `Executable doesn't exist at /opt/ms-playwright/chromium_headless_shell-1248/...`.
The other tiers in the same run: 1033 / 4 / 19 / 224. Run separately: **L5 OK, L6 OK** (`cargo-audit-passed` on 85b12793,
`pnpm-audit-passed`, `buf-breaking-passed`). Browser tiers via the scratch Chromium: sdk-svelte 27/27, web-app 113/113.

---

## Gate 3 — Verdicts

| Reviewer | Verdict | Findings | Fixed | Deferred | Notes |
|----------|---------|----------|-------|----------|-------|
| Security | RESOLVED-FIXED (re-confirmed after O-1) | 1 | 1 | 0 | offline-by-construction via `setupFiles` (operator-approved config exception); browser-tier assertions re-confirm after image rebuild |
| Test | RESOLVED-FIXED | 2 | 2 | 0 | F1 diag.disable pinned (+ positive control); F2 fixed by tier-wide setupFiles mock; counts/names/coverage independently verified |
| Observability | RESOLVED-FIXED | 1 | 1 | 0 | O-1 allSettled shutdowns + always reset globals + rethrow; dup-registration 0 all tiers (`--reporter=default`) |
| Code Quality | RESOLVED-FIXED | 7 | 7 | 0 | F1=O-1; F2 setupFiles cleanup; F4=D-1; F3,F5–F7 small |
| DRY | RESOLVED-FIXED | 1 | 1 | 0 | D-1 shared `NX_PIPELINE_OUTPUT_STYLE`; extraction opportunity (pre-existing repeated pins) in docs/TODO.md, not a deferral per 08a6aafb |
| Operations | RESOLVED-FIXED | 2 | 2 | 0 | runbook example arrow; `.vite` cache in rollback; pnpm-peers offline + red-on-#78 verified |

**Gate 3 PASSED** (2026-10-21): all six reviewers are RESOLVED-FIXED, with 0 accepted deferrals.

### Resume point (for `--continue`)
The container's Playwright browser is stale (chromium_headless_shell-1234 vs the 1248 that playwright 1.64.1 needs), so Gate 2 can't pass in this container. The operator recreates it: `infra/devloop/devloop.sh --rebuild --recreate <task-slug>`. The resumed devloop then:
1. `git worktree prune` (the scratch worktrees under the old container's /tmp are gone);
2. checks `ls /opt/ms-playwright` shows a `chromium_headless_shell-1248`;
3. runs Gate 2: `DEVLOOP_FMT_APPLY=1 ./scripts/layer-all.sh`, all seven layers, including the browser tiers and Layer 7 on the rebuilt image. Expected L4 counts: test-utils 19, sdk-core unit 1034, sdk-core component 4, web-app unit 224, sdk-svelte browser 27, web-app browser 113;
4. on a pass, sets Phase `complete` and commits (Step 8) with the breaking-changes list recorded in this file.
Any Gate 2 failure goes back to implementation with a respawned roster. Gate 1 and Gate 3 are not re-run unless the tree changes.

Commits already on the branch ahead of this devloop's commit: `85b12793` (Cargo.lock RUSTSEC-2026-0381/0382/0383, separate revert unit) and `08a6aafb` (review-protocol: extraction opportunities don't force RESOLVED-DEFERRED).
Remaining operator actions after merge: close Dependabot PR #78; confirm the first Dependabot npm-otel PR bumps the `catalog:` entry.

---

## Human Review (Iteration 2)

**Feedback**: "run Gate 2 and commit"

Resumed via `--continue` after the operator rebuilt the devloop image. Tree unchanged since Gate 3, so Gate 1 / Gate 3 were not re-run and no roster was respawned.
- `git worktree prune`: only `/work` remains.
- `/opt/ms-playwright` now holds `chromium_headless_shell-1248` (Issue 1 resolved by the host rebuild).

## Gate 2 — Full Validation

`DEVLOOP_FMT_APPLY=1 ./scripts/layer-all.sh` (first attempt): **`TOTAL_RESULT=OK`**, all seven layers run, no `FMT_APPLIED`.

| Layer | Result | Duration (s) | Notes |
|-------|--------|--------------|-------|
| 1 Compile | OK | 10 | incl. `pnpm-peers-passed`, `nx-typecheck-passed` |
| 2 Format | OK | 4 | |
| 3 Guards | OK | 139 | `WARN BUDGET_BREACH` (budget 20s), not a failure |
| 4 Test | OK | 157 | test-utils 19, sdk-core unit 1034, sdk-core component 4, sdk-svelte browser 27, web-app unit 224, web-app browser 113 — all match the expected counts |
| 5 Lint | OK | 7 | |
| 6 Audit | OK | 8 | `cargo-audit-passed`, `pnpm-audit-passed`, `buf-breaking-passed` |
| 7 Env-tests | OK | 1156 | `env-tests-passed`, `browser-e2e-passed` |

Security's browser-tier re-confirmation note (Gate 3) is satisfied by the L4 browser tiers above running on the real image.

